//! `agent-sandbox` — sandbox assessment boundary.
//!
//! This crate records what sandbox enforcement the harness can claim for a
//! tool call. The current native runners approval-gate high-risk permissions,
//! but most OS-level isolation from `specs/architecture.md` §17 is still
//! advisory. The trace should say that plainly.

use agent_tools::ToolPermissions;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxEnforcementLevel {
    NotRequired,
    Partial,
    Advisory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxPermissionReport {
    pub permission: String,
    pub requested: bool,
    pub level: SandboxEnforcementLevel,
    pub mechanism: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxReport {
    pub platform: String,
    pub overall_level: SandboxEnforcementLevel,
    pub permissions: Vec<SandboxPermissionReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

pub fn assess_permissions(permissions: &ToolPermissions) -> SandboxReport {
    let mut entries = vec![
        shell_entry(permissions),
        permission_entry(
            "file_read",
            permissions.file_read,
            "declared permission plus approval gate; process environment is scrubbed for native process tools",
            "file-read scope is declared and approval-gated, but bind-mount/chroot-equivalent enforcement is not active in this runner",
            SandboxEnforcementLevel::Advisory,
        ),
        permission_entry(
            "file_write",
            permissions.file_write,
            "declared permission plus approval gate",
            "file-write scope is declared and approval-gated, but writable path isolation is not active in this runner",
            SandboxEnforcementLevel::Advisory,
        ),
        permission_entry(
            "network",
            permissions.network,
            "declared permission plus approval gate",
            "network access is declared and approval-gated, but per-tool egress isolation is not active in this runner",
            SandboxEnforcementLevel::Advisory,
        ),
        permission_entry(
            "secrets",
            permissions.secrets,
            "secret-handle resolution with trace redaction",
            "secret values are kept out of LLM prompts and traces; inspect the active secret backend to verify whether OS keychain storage is in use",
            SandboxEnforcementLevel::Partial,
        ),
        permission_entry(
            "wallet",
            permissions.wallet,
            "declared permission plus approval gate",
            "wallet access is approval-gated, but no dedicated wallet sandbox is active in this runner",
            SandboxEnforcementLevel::Advisory,
        ),
        permission_entry(
            "payment",
            permissions.payment,
            "declared permission plus approval gate",
            "payment access is approval-gated, but no payment-spend sandbox is active in this runner",
            SandboxEnforcementLevel::Advisory,
        ),
        permission_entry(
            "browser_profile",
            permissions.browser_profile,
            "declared permission plus approval gate",
            "browser-profile access is approval-gated, but no browser-profile vault isolation is active in this runner",
            SandboxEnforcementLevel::Advisory,
        ),
    ];
    entries.sort_by(|a, b| a.permission.cmp(&b.permission));
    let warnings = entries
        .iter()
        .filter_map(|entry| entry.warning.clone())
        .collect::<Vec<_>>();
    let overall_level = entries
        .iter()
        .filter(|entry| entry.requested)
        .map(|entry| entry.level)
        .max()
        .unwrap_or(SandboxEnforcementLevel::NotRequired);

    SandboxReport {
        platform: std::env::consts::OS.to_string(),
        overall_level,
        permissions: entries,
        warnings,
    }
}

fn permission_entry(
    permission: &str,
    requested: bool,
    mechanism: &str,
    warning: &str,
    requested_level: SandboxEnforcementLevel,
) -> SandboxPermissionReport {
    SandboxPermissionReport {
        permission: permission.into(),
        requested,
        level: if requested {
            requested_level
        } else {
            SandboxEnforcementLevel::NotRequired
        },
        mechanism: if requested {
            mechanism.into()
        } else {
            "permission not requested".into()
        },
        warning: requested.then(|| warning.into()),
    }
}

fn shell_entry(permissions: &ToolPermissions) -> SandboxPermissionReport {
    if !permissions.shell {
        return permission_entry(
            "shell",
            false,
            "permission not requested",
            "",
            SandboxEnforcementLevel::NotRequired,
        );
    }
    if permissions.shell_restricted {
        SandboxPermissionReport {
            permission: "shell".into(),
            requested: true,
            level: SandboxEnforcementLevel::Partial,
            mechanism: "direct command execution through an allowlisted command set with a minimal inherited environment".into(),
            warning: Some(
                "shell command names and shell metacharacters are restricted, but full OS process sandboxing is not active"
                    .into(),
            ),
        }
    } else {
        permission_entry(
            "shell",
            true,
            "approval gate before native shell execution with a minimal inherited environment",
            "shell execution is approval-gated and environment-scrubbed, but the native shell runner does not yet enforce an OS command allowlist or process sandbox",
            SandboxEnforcementLevel::Advisory,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_requested_permissions_need_no_sandbox() {
        let report = assess_permissions(&ToolPermissions::default());

        assert_eq!(report.overall_level, SandboxEnforcementLevel::NotRequired);
        assert!(report.warnings.is_empty());
        assert!(report.permissions.iter().all(|entry| !entry.requested));
    }

    #[test]
    fn shell_permission_is_reported_as_advisory() {
        let report = assess_permissions(&ToolPermissions {
            shell: true,
            ..ToolPermissions::default()
        });

        assert_eq!(report.overall_level, SandboxEnforcementLevel::Advisory);
        assert!(report.warnings.iter().any(|item| item.contains("shell")));
    }

    #[test]
    fn restricted_shell_permission_is_reported_as_partial() {
        let report = assess_permissions(&ToolPermissions {
            shell: true,
            shell_restricted: true,
            ..ToolPermissions::default()
        });

        assert_eq!(report.overall_level, SandboxEnforcementLevel::Partial);
        assert!(
            report
                .warnings
                .iter()
                .any(|item| item.contains("metacharacters"))
        );
    }

    #[test]
    fn secret_permission_reports_partial_redaction_boundary() {
        let report = assess_permissions(&ToolPermissions {
            secrets: true,
            ..ToolPermissions::default()
        });

        assert_eq!(report.overall_level, SandboxEnforcementLevel::Partial);
        assert!(
            report
                .permissions
                .iter()
                .any(|entry| entry.permission == "secrets" && entry.requested)
        );
    }
}
