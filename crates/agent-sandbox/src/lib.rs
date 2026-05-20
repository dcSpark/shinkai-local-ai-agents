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
pub struct SandboxProfilePlan {
    pub platform: String,
    pub profile_kind: String,
    pub mechanism: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command_prefix: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxReport {
    pub platform: String,
    pub overall_level: SandboxEnforcementLevel,
    pub permissions: Vec<SandboxPermissionReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_plan: Option<SandboxProfilePlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

pub fn assess_permissions(permissions: &ToolPermissions) -> SandboxReport {
    let platform = std::env::consts::OS.to_string();
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
        profile_plan: sandbox_profile_plan_for_platform(&platform, permissions),
        platform,
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

fn sandbox_profile_plan_for_platform(
    platform: &str,
    permissions: &ToolPermissions,
) -> Option<SandboxProfilePlan> {
    if !has_requested_permission(permissions) {
        return None;
    }

    Some(match platform {
        "macos" => macos_sandbox_exec_plan(permissions),
        "linux" => linux_bubblewrap_plan(permissions),
        "windows" => windows_restricted_token_plan(permissions),
        other => generic_sandbox_plan(other),
    })
}

fn has_requested_permission(permissions: &ToolPermissions) -> bool {
    permissions.shell
        || permissions.file_read
        || permissions.file_write
        || permissions.network
        || permissions.secrets
        || permissions.wallet
        || permissions.payment
        || permissions.browser_profile
}

fn macos_sandbox_exec_plan(permissions: &ToolPermissions) -> SandboxProfilePlan {
    let mut profile = vec![
        "(version 1)".to_string(),
        "(deny default)".to_string(),
        "(allow process*)".to_string(),
        "(allow signal*)".to_string(),
        "(allow sysctl-read)".to_string(),
        "(allow file-read* (literal \"/dev/null\"))".to_string(),
    ];

    if permissions.file_read || permissions.shell {
        profile.push("(allow file-read* (subpath \"<read-scope>\"))".to_string());
    }
    if permissions.file_write {
        profile.push("(allow file-write* (subpath \"<write-scope>\"))".to_string());
    }
    if permissions.network {
        profile.push("(allow network*)".to_string());
    } else {
        profile.push("(deny network*)".to_string());
    }
    if permissions.secrets {
        profile.push(
            "; inject only resolved SecretHandle env vars; do not expose secret backing store"
                .to_string(),
        );
    }

    SandboxProfilePlan {
        platform: "macos".into(),
        profile_kind: "sandbox-exec-template".into(),
        mechanism: "macOS sandbox-exec profile template for filesystem and network posture".into(),
        command_prefix: vec![
            "sandbox-exec".into(),
            "-p".into(),
            "<generated-profile>".into(),
        ],
        profile: Some(profile.join("\n")),
        notes: profile_plan_notes(
            "Replace placeholder path scopes with resolved read/write mounts before enforcement.",
        ),
    }
}

fn linux_bubblewrap_plan(permissions: &ToolPermissions) -> SandboxProfilePlan {
    let mut command_prefix = vec![
        "bwrap".to_string(),
        "--die-with-parent".to_string(),
        "--new-session".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
    ];

    if !permissions.network {
        command_prefix.push("--unshare-net".to_string());
    }
    if permissions.file_read || permissions.shell {
        command_prefix.extend([
            "--ro-bind".to_string(),
            "<read-scope>".to_string(),
            "<read-scope>".to_string(),
        ]);
    }
    if permissions.file_write {
        command_prefix.extend([
            "--bind".to_string(),
            "<write-scope>".to_string(),
            "<write-scope>".to_string(),
        ]);
    }
    command_prefix.extend([
        "--chdir".to_string(),
        "<workdir>".to_string(),
        "--".to_string(),
        "<tool-command>".to_string(),
    ]);

    SandboxProfilePlan {
        platform: "linux".into(),
        profile_kind: "bubblewrap-plan".into(),
        mechanism:
            "Linux bubblewrap/rootless namespace command plan for filesystem and network posture"
                .into(),
        command_prefix,
        profile: None,
        notes: profile_plan_notes(
            "Bind concrete executable, library, read, write, and workdir scopes before enforcement.",
        ),
    }
}

fn windows_restricted_token_plan(_permissions: &ToolPermissions) -> SandboxProfilePlan {
    SandboxProfilePlan {
        platform: "windows".into(),
        profile_kind: "restricted-token-job-wfp-plan".into(),
        mechanism: "Windows restricted token plus Job Object and WFP child-process egress plan"
            .into(),
        command_prefix: Vec::new(),
        profile: None,
        notes: profile_plan_notes(
            "Create a restricted token, assign the process to a Job Object, and add WFP egress rules for the child process.",
        ),
    }
}

fn generic_sandbox_plan(platform: &str) -> SandboxProfilePlan {
    SandboxProfilePlan {
        platform: platform.into(),
        profile_kind: "manual-sandbox-plan".into(),
        mechanism: "No built-in OS profile template is available for this platform".into(),
        command_prefix: Vec::new(),
        profile: None,
        notes: profile_plan_notes(
            "Use an external process boundary that matches the declared tool permissions.",
        ),
    }
}

fn profile_plan_notes(platform_note: &str) -> Vec<String> {
    vec![
        "Profile plan is emitted for traceability; native runners do not automatically apply it per tool call yet."
            .into(),
        platform_note.into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_requested_permissions_need_no_sandbox() {
        let report = assess_permissions(&ToolPermissions::default());

        assert_eq!(report.overall_level, SandboxEnforcementLevel::NotRequired);
        assert!(report.warnings.is_empty());
        assert!(report.profile_plan.is_none());
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

    #[test]
    fn macos_profile_plan_denies_network_when_not_requested() {
        let plan = sandbox_profile_plan_for_platform(
            "macos",
            &ToolPermissions {
                file_read: true,
                ..ToolPermissions::default()
            },
        )
        .expect("profile plan");

        assert_eq!(plan.profile_kind, "sandbox-exec-template");
        assert_eq!(
            plan.command_prefix.first().map(String::as_str),
            Some("sandbox-exec")
        );
        assert!(
            plan.profile
                .as_deref()
                .is_some_and(|profile| profile.contains("(deny network*)"))
        );
    }

    #[test]
    fn linux_profile_plan_unshares_network_when_not_requested() {
        let plan = sandbox_profile_plan_for_platform(
            "linux",
            &ToolPermissions {
                file_read: true,
                ..ToolPermissions::default()
            },
        )
        .expect("profile plan");

        assert_eq!(plan.profile_kind, "bubblewrap-plan");
        assert!(plan.command_prefix.iter().any(|arg| arg == "--unshare-net"));
    }

    #[test]
    fn linux_profile_plan_keeps_network_when_requested() {
        let plan = sandbox_profile_plan_for_platform(
            "linux",
            &ToolPermissions {
                network: true,
                ..ToolPermissions::default()
            },
        )
        .expect("profile plan");

        assert_eq!(plan.profile_kind, "bubblewrap-plan");
        assert!(!plan.command_prefix.iter().any(|arg| arg == "--unshare-net"));
    }

    #[test]
    fn windows_profile_plan_records_restricted_token_job_and_wfp() {
        let plan = sandbox_profile_plan_for_platform(
            "windows",
            &ToolPermissions {
                file_write: true,
                ..ToolPermissions::default()
            },
        )
        .expect("profile plan");

        assert_eq!(plan.profile_kind, "restricted-token-job-wfp-plan");
        assert!(plan.mechanism.contains("restricted token"));
        assert!(
            plan.notes
                .iter()
                .any(|note| note.contains("Job Object") && note.contains("WFP"))
        );
    }
}
