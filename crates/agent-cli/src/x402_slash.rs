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
     - /x402 request <url> [--method GET|POST] [--max-amount n] [--auto-pay] [--signature-secret id]\n\
     - /x402 required --resource <url> --amount <n> --pay-to <addr> --asset <asset> --network <name>\n\
     - /x402 settle <payment-signature> --facilitator <url> --resource <url> --amount <n> --pay-to <addr> --asset <asset> --network <name> [--mode verify|settle|verify-and-settle]\n\
     /payment x402-request, /payment x402-required, and /payment x402-settle are aliases."
}

pub(crate) fn is_help(rest: &str) -> bool {
    let rest = rest.trim();
    matches!(rest, "help" | "--help")
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
            "x402 command needs request, required, settle, or help"
        )),
    }
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
}
