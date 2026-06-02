pub(crate) fn slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/x402" || trimmed == "/payment" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/x402 ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/payment ").map(str::trim)
    }
}

pub(crate) fn help_text() -> &'static str {
    "x402 shortcuts:\n\
     - /x402 status - inspect redacted local x402 payment readiness\n\
     - /x402 request <url> [--method GET|POST] [--max-amount n] [--auto-pay] [--signature-secret id]\n\
     - /x402 required --resource <url> --amount <n> --pay-to <addr> --asset <asset> --network <name>\n\
     - /x402 settle <payment-signature> --facilitator <url> --resource <url> --amount <n> --pay-to <addr> --asset <asset> --network <name> [--mode verify|settle|verify-and-settle]\n\
     /payment x402-status, /payment x402-request, /payment x402-required, and /payment x402-settle are aliases."
}

pub(crate) fn is_help(rest: &str) -> bool {
    let rest = rest.trim();
    matches!(rest, "help" | "--help")
}

pub(crate) fn is_status(rest: &str) -> bool {
    let rest = rest.trim();
    matches!(rest, "status" | "x402-status")
}

pub(crate) fn status_json() -> serde_json::Value {
    status_json_from_lookup(|key| std::env::var(key).ok())
}

pub(crate) fn status_text() -> String {
    let status = status_json();
    let signature_env = status["signature_env_name"]
        .as_str()
        .unwrap_or("AGENT_X402_PAYMENT_SIGNATURE");
    let wallet_args = status["wallet_args_count"].as_u64().unwrap_or_default();
    [
        "x402 payment status".to_string(),
        format!(
            "payment tools: {}",
            bool_label(status["payment_tools_enabled"].as_bool().unwrap_or(false))
        ),
        format!(
            "retry spend limit: {}",
            bool_label(status["max_amount_configured"].as_bool().unwrap_or(false))
        ),
        format!(
            "signature env {signature_env}: {}",
            bool_label(status["signature_env_configured"].as_bool().unwrap_or(false))
        ),
        format!(
            "signature secret handle: {}",
            bool_label(status["signature_secret_configured"].as_bool().unwrap_or(false))
        ),
        format!(
            "wallet command: {} (args {wallet_args}, timeout {})",
            bool_label(status["wallet_command_configured"].as_bool().unwrap_or(false)),
            bool_label(status["wallet_timeout_configured"].as_bool().unwrap_or(false))
        ),
        format!(
            "facilitator: {}",
            bool_label(status["facilitator_url_configured"].as_bool().unwrap_or(false))
        ),
        format!(
            "auto-pay ready: {}",
            bool_label(status["auto_pay_ready"].as_bool().unwrap_or(false))
        ),
        format!(
            "settlement ready: {}",
            bool_label(status["settlement_ready"].as_bool().unwrap_or(false))
        ),
    ]
    .join("\n")
}

pub(crate) fn parse_tool_call(rest: &str) -> anyhow::Result<(&'static str, serde_json::Value)> {
    let rest = rest.trim();
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "request" | "x402-request" => {
            parse_request(args).map(|input| ("payment_x402_request", input))
        }
        "required" | "x402-required" => {
            parse_required(args).map(|input| ("payment_x402_required", input))
        }
        "settle" | "x402-settle" => parse_settle(args).map(|input| ("payment_x402_settle", input)),
        _ => Err(anyhow::anyhow!(
            "x402 command needs status, request, required, settle, or help"
        )),
    }
}

fn status_json_from_lookup<F>(lookup: F) -> serde_json::Value
where
    F: Fn(&str) -> Option<String>,
{
    let env = |key: &str| lookup(key).and_then(clean_non_empty);
    let signature_env = env("AGENT_X402_SIGNATURE_ENV")
        .unwrap_or_else(|| "AGENT_X402_PAYMENT_SIGNATURE".into());
    let signature_env_configured = env(&signature_env).is_some();
    let signature_secret_configured = env("AGENT_X402_SIGNATURE_SECRET").is_some();
    let wallet_command_configured = env("AGENT_X402_WALLET_COMMAND").is_some();
    let wallet_args_count = env("AGENT_X402_WALLET_ARGS_JSON")
        .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
        .map_or(0, |args| args.len());
    let payment_tools_enabled = env_flag_lookup(&lookup, "AGENT_PAYMENT_TOOLS");
    let max_amount_configured = env("AGENT_PAYMENT_MAX_AMOUNT").is_some();
    let facilitator_url_configured = env("AGENT_X402_FACILITATOR_URL").is_some();
    serde_json::json!({
        "payment_tools_enabled": payment_tools_enabled,
        "max_amount_configured": max_amount_configured,
        "max_response_bytes_configured": env("AGENT_PAYMENT_MAX_RESPONSE_BYTES").is_some(),
        "timeout_ms_configured": env("AGENT_PAYMENT_TIMEOUT_MS").is_some(),
        "signature_env_name": signature_env,
        "signature_env_configured": signature_env_configured,
        "signature_secret_configured": signature_secret_configured,
        "facilitator_url_configured": facilitator_url_configured,
        "wallet_command_configured": wallet_command_configured,
        "wallet_args_configured": wallet_args_count > 0,
        "wallet_args_count": wallet_args_count,
        "wallet_timeout_configured": env("AGENT_X402_WALLET_TIMEOUT_MS").is_some(),
        "auto_pay_ready": payment_tools_enabled
            && max_amount_configured
            && (signature_env_configured || signature_secret_configured || wallet_command_configured),
        "settlement_ready": payment_tools_enabled && facilitator_url_configured
    })
}

fn env_flag_lookup<F>(lookup: &F, key: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key)
        .and_then(clean_non_empty)
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

fn clean_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn bool_label(value: bool) -> &'static str {
    if value { "configured" } else { "missing" }
}

fn parse_request(rest: &str) -> anyhow::Result<serde_json::Value> {
    let mut parts = rest.split_whitespace().peekable();
    let url = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("x402 request shortcut needs a URL"))?;
    let mut input = serde_json::Map::new();
    input.insert("url".into(), serde_json::Value::String(url.into()));
    while let Some(part) = parts.next() {
        if part == "--auto-pay" {
            input.insert("auto_pay".into(), serde_json::Value::Bool(true));
            continue;
        }
        if let Some((flag, value)) = option_value(part, &mut parts)? {
            match flag {
                "--method" => {
                    let method = value.to_ascii_uppercase();
                    if !matches!(method.as_str(), "GET" | "POST") {
                        anyhow::bail!("x402 request --method must be GET or POST");
                    }
                    input.insert("method".into(), serde_json::Value::String(method));
                }
                "--max-amount" => {
                    let amount = parse_non_negative_number(value, "x402 request --max-amount")?;
                    input.insert("max_amount".into(), amount);
                }
                "--signature-secret" => {
                    input.insert(
                        "payment_signature_secret".into(),
                        serde_json::Value::String(value.into()),
                    );
                }
                _ => anyhow::bail!("unknown x402 request option: {flag}"),
            }
        } else {
            anyhow::bail!("unknown x402 request option: {part}");
        }
    }
    Ok(serde_json::Value::Object(input))
}

fn parse_required(rest: &str) -> anyhow::Result<serde_json::Value> {
    let mut parts = rest.split_whitespace().peekable();
    let mut input = serde_json::Map::new();
    let mut accept = default_accept();
    while let Some(part) = parts.next() {
        let Some((flag, value)) = option_value(part, &mut parts)? else {
            anyhow::bail!("unknown x402 required option: {part}");
        };
        match flag {
            "--version" => {
                let version = parse_positive_u64(value, "x402 required --version")?;
                input.insert(
                    "x402_version".into(),
                    serde_json::Value::Number(version.into()),
                );
            }
            "--error" => {
                input.insert("error".into(), serde_json::Value::String(value.into()));
            }
            "--body" => {
                input.insert("body".into(), serde_json::Value::String(value.into()));
            }
            "--scheme" | "--network" | "--amount" | "--max-amount" | "--pay-to" | "--asset"
            | "--resource" => set_accept_option(&mut accept, flag, value),
            _ => anyhow::bail!("unknown x402 required option: {flag}"),
        }
    }
    validate_accept(&accept, "x402 required")?;
    input.insert(
        "accepts".into(),
        serde_json::Value::Array(vec![serde_json::Value::Object(accept)]),
    );
    Ok(serde_json::Value::Object(input))
}

fn parse_settle(rest: &str) -> anyhow::Result<serde_json::Value> {
    let mut parts = rest.split_whitespace().peekable();
    let signature = parts
        .next()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("x402 settle shortcut needs a PAYMENT-SIGNATURE value"))?;
    let mut input = serde_json::Map::new();
    input.insert(
        "payment_signature".into(),
        serde_json::Value::String(signature.into()),
    );
    let mut accept = default_accept();
    while let Some(part) = parts.next() {
        let Some((flag, value)) = option_value(part, &mut parts)? else {
            anyhow::bail!("unknown x402 settle option: {part}");
        };
        match flag {
            "--facilitator" | "--facilitator-url" => {
                input.insert(
                    "facilitator_url".into(),
                    serde_json::Value::String(value.into()),
                );
            }
            "--mode" => {
                let mode = value.replace('-', "_");
                if !matches!(mode.as_str(), "verify" | "settle" | "verify_and_settle") {
                    anyhow::bail!(
                        "x402 settle --mode must be verify, settle, or verify-and-settle"
                    );
                }
                input.insert("mode".into(), serde_json::Value::String(mode));
            }
            "--version" => {
                let version = parse_positive_u64(value, "x402 settle --version")?;
                input.insert(
                    "x402_version".into(),
                    serde_json::Value::Number(version.into()),
                );
            }
            "--scheme" | "--network" | "--amount" | "--max-amount" | "--pay-to" | "--asset"
            | "--resource" => set_accept_option(&mut accept, flag, value),
            _ => anyhow::bail!("unknown x402 settle option: {flag}"),
        }
    }
    if !input.contains_key("facilitator_url") {
        anyhow::bail!("x402 settle needs --facilitator <url>");
    }
    validate_accept(&accept, "x402 settle")?;
    input.insert(
        "payment_requirements".into(),
        serde_json::Value::Object(accept),
    );
    Ok(serde_json::Value::Object(input))
}

fn option_value<'a, I>(
    part: &'a str,
    parts: &mut std::iter::Peekable<I>,
) -> anyhow::Result<Option<(&'a str, &'a str)>>
where
    I: Iterator<Item = &'a str>,
{
    if let Some((flag, value)) = part.split_once('=') {
        if value.trim().is_empty() {
            anyhow::bail!("{flag} needs a value");
        }
        return Ok(Some((flag, value.trim())));
    }
    if !part.starts_with("--") {
        return Ok(None);
    }
    let value = parts
        .next()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("{part} needs a value"))?;
    Ok(Some((part, value)))
}

fn default_accept() -> serde_json::Map<String, serde_json::Value> {
    let mut accept = serde_json::Map::new();
    accept.insert("scheme".into(), serde_json::Value::String("exact".into()));
    accept
}

fn set_accept_option(
    accept: &mut serde_json::Map<String, serde_json::Value>,
    flag: &str,
    value: &str,
) {
    let key = match flag {
        "--scheme" => "scheme",
        "--network" => "network",
        "--amount" | "--max-amount" => "maxAmountRequired",
        "--pay-to" => "payTo",
        "--asset" => "asset",
        "--resource" => "resource",
        _ => return,
    };
    accept.insert(key.into(), serde_json::Value::String(value.into()));
}

fn validate_accept(
    accept: &serde_json::Map<String, serde_json::Value>,
    label: &str,
) -> anyhow::Result<()> {
    let missing = [
        ("--resource", "resource"),
        ("--amount", "maxAmountRequired"),
        ("--pay-to", "payTo"),
        ("--asset", "asset"),
        ("--network", "network"),
    ]
    .into_iter()
    .filter_map(|(flag, key)| {
        let present = accept
            .get(key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
        (!present).then_some(flag)
    })
    .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{label} needs {}", missing.join(", "))
    }
}

fn parse_non_negative_number(value: &str, label: &str) -> anyhow::Result<serde_json::Value> {
    let amount = value
        .parse::<f64>()
        .map_err(|_| anyhow::anyhow!("{label} needs a non-negative number"))?;
    if !amount.is_finite() || amount < 0.0 {
        anyhow::bail!("{label} needs a non-negative number");
    }
    let number = serde_json::Number::from_f64(amount)
        .ok_or_else(|| anyhow::anyhow!("{label} needs a non-negative number"))?;
    Ok(serde_json::Value::Number(number))
}

fn parse_positive_u64(value: &str, label: &str) -> anyhow::Result<u64> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("{label} needs a positive integer"))?;
    if parsed == 0 {
        anyhow::bail!("{label} needs a positive integer");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_x402_and_payment_prefixes() {
        assert_eq!(
            slash_rest("/x402 request https://example.test"),
            Some("request https://example.test")
        );
        assert_eq!(
            slash_rest("/payment x402-request https://example.test"),
            Some("x402-request https://example.test")
        );
        assert_eq!(slash_rest("/payments"), None);
        assert!(is_status("status"));
        assert!(is_status("x402-status"));
    }

    #[test]
    fn builds_native_tool_inputs() {
        assert_eq!(
            parse_tool_call(
                "request https://example.test --method post --max-amount=5 --auto-pay --signature-secret sig"
            )
            .unwrap(),
            (
                "payment_x402_request",
                json!({
                    "url": "https://example.test",
                    "method": "POST",
                    "max_amount": 5.0,
                    "auto_pay": true,
                    "payment_signature_secret": "sig"
                })
            )
        );
        assert_eq!(
            parse_tool_call(
                "required --resource https://example.test --amount 5 --pay-to 0xabc --asset USDC --network base-sepolia --version=2"
            )
            .unwrap(),
            (
                "payment_x402_required",
                json!({
                    "x402_version": 2,
                    "accepts": [{
                        "scheme": "exact",
                        "resource": "https://example.test",
                        "maxAmountRequired": "5",
                        "payTo": "0xabc",
                        "asset": "USDC",
                        "network": "base-sepolia"
                    }]
                })
            )
        );
        assert_eq!(
            parse_tool_call(
                "settle signature --facilitator http://127.0.0.1:8787 --resource https://example.test --amount 5 --pay-to 0xabc --asset USDC --network base-sepolia --mode verify-and-settle"
            )
            .unwrap(),
            (
                "payment_x402_settle",
                json!({
                    "payment_signature": "signature",
                    "facilitator_url": "http://127.0.0.1:8787",
                    "mode": "verify_and_settle",
                    "payment_requirements": {
                        "scheme": "exact",
                        "resource": "https://example.test",
                        "maxAmountRequired": "5",
                        "payTo": "0xabc",
                        "asset": "USDC",
                        "network": "base-sepolia"
                    }
                })
            )
        );
    }

    #[test]
    fn rejects_incomplete_inputs() {
        assert!(parse_tool_call("request").is_err());
        assert!(parse_tool_call("required --resource only").is_err());
        assert!(parse_tool_call("settle signature").is_err());
    }

    #[test]
    fn reports_redacted_payment_status() {
        let status = status_json_from_lookup(|key| match key {
            "AGENT_PAYMENT_TOOLS" => Some("1".into()),
            "AGENT_PAYMENT_MAX_AMOUNT" => Some("10".into()),
            "AGENT_X402_SIGNATURE_ENV" => Some("CUSTOM_PAYMENT_SIGNATURE".into()),
            "CUSTOM_PAYMENT_SIGNATURE" => Some("secret-signature".into()),
            "AGENT_X402_WALLET_COMMAND" => Some("/bin/wallet".into()),
            "AGENT_X402_WALLET_ARGS_JSON" => Some(r#"["sign"]"#.into()),
            "AGENT_X402_FACILITATOR_URL" => Some("https://facilitator.example".into()),
            _ => None,
        });
        assert_eq!(status["payment_tools_enabled"], true);
        assert_eq!(status["signature_env_name"], "CUSTOM_PAYMENT_SIGNATURE");
        assert_eq!(status["signature_env_configured"], true);
        assert_eq!(status["wallet_args_count"], 1);
        assert_eq!(status["auto_pay_ready"], true);
        assert_eq!(status["settlement_ready"], true);
        assert!(!status.to_string().contains("secret-signature"));
    }
}
