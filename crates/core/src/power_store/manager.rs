//! Package managers PowerStore knows about.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ManagerKind {
    WinGet,
    Scoop,
    Chocolatey,
    Npm,
    Pip,
    Cargo,
    DotnetTool,
    PowerShell,
    PowerShell7,
    Vcpkg,
}

impl ManagerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WinGet => "winget",
            Self::Scoop => "scoop",
            Self::Chocolatey => "chocolatey",
            Self::Npm => "npm",
            Self::Pip => "pip",
            Self::Cargo => "cargo",
            Self::DotnetTool => "dotnet",
            Self::PowerShell => "powershell",
            Self::PowerShell7 => "powershell7",
            Self::Vcpkg => "vcpkg",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "winget" => Self::WinGet,
            "scoop" => Self::Scoop,
            "chocolatey" => Self::Chocolatey,
            "npm" => Self::Npm,
            "pip" => Self::Pip,
            "cargo" => Self::Cargo,
            "dotnet" => Self::DotnetTool,
            "powershell" => Self::PowerShell,
            "powershell7" => Self::PowerShell7,
            "vcpkg" => Self::Vcpkg,
            _ => return None,
        })
    }

    /// CLI managers can run from a user-chosen program; WinGet uses its COM API instead.
    pub fn supports_executable(self) -> bool {
        !matches!(self, Self::WinGet)
    }

    /// Managers whose operations honor an install scope (see [`InstallOptions::scope`]).
    pub fn supports_default_scope(self) -> bool {
        matches!(self, Self::WinGet)
    }

    /// Managers whose operations honor an installer architecture (see
    /// [`InstallOptions::architecture`]).
    pub fn supports_default_architecture(self) -> bool {
        matches!(self, Self::WinGet)
    }

    /// vcpkg builds every package for a triplet (one of [`VCPKG_TRIPLETS`]).
    pub fn supports_default_triplet(self) -> bool {
        matches!(self, Self::Vcpkg)
    }

    /// Managers whose install and update operations honor [`InstallOptions::version`].
    pub fn supports_version(self) -> bool {
        matches!(
            self,
            Self::WinGet | Self::Scoop | Self::Chocolatey | Self::Pip
        )
    }

    /// Managers that pass [`InstallOptions::extra_args`] to their command line.
    pub fn supports_extra_args(self) -> bool {
        matches!(self, Self::WinGet | Self::Chocolatey)
    }

    /// Whether the package options editor has any field this manager honors.
    pub fn supports_install_options(self) -> bool {
        self.supports_default_scope()
            || self.supports_default_architecture()
            || self.supports_version()
            || self.supports_extra_args()
    }

    /// What to run or open to get the manager when it is missing. PowerStore never
    /// installs a manager itself; the settings drawer only shows this text for copying.
    pub fn install_hint(self) -> Option<&'static str> {
        match self {
            Self::WinGet => Some("https://aka.ms/getwinget"),
            Self::Scoop => Some("irm get.scoop.sh | iex"),
            Self::Chocolatey => Some("https://chocolatey.org/install"),
            Self::Npm => Some("winget install OpenJS.NodeJS.LTS"),
            Self::Pip => Some("winget install Python.Python.3.13"),
            Self::Cargo => Some("winget install Rustlang.Rustup"),
            Self::DotnetTool => Some("winget install Microsoft.DotNet.SDK.10"),
            // Windows PowerShell ships with Windows.
            Self::PowerShell => None,
            Self::PowerShell7 => Some("winget install Microsoft.PowerShell"),
            Self::Vcpkg => Some("https://learn.microsoft.com/vcpkg/get_started/get-started"),
        }
    }
}
