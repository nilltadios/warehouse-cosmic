// SPDX-License-Identifier: GPL-3.0-or-later
//! The running GUI's desktop D-Bus API. No MCP or separate service is needed.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

pub const PREFIX: &str = "dev.nilltadios.WarehouseCosmic.UI.";
pub const PATH: &str = "/dev/nilltadios/WarehouseCosmic/UI";
pub const INTERFACE: &str = "dev.nilltadios.WarehouseCosmic.UI";

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub request: Request,
    #[serde(default)]
    pub apply: bool,
    #[serde(default)]
    pub allow_system: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "command",
    content = "params",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Request {
    #[default]
    Inspect,
    Tree,
    Present,
    ActivationToken,
    Activate {
        target: String,
    },
    Open {
        files: Vec<PathBuf>,
    },
    Theme {
        theme: String,
    },
    Window {
        width: Option<u32>,
        height: Option<u32>,
        mode: Option<String>,
    },
    Capture {
        path: PathBuf,
        #[serde(default)]
        overwrite: bool,
    },
    Fill {
        target: String,
        text: String,
    },
    Resolve {
        target: String,
    },
    Close,
}
pub fn success(data: Value) -> Value {
    json!({"schema":"warehouse-cosmic-ui/1", "ok":true, "data":data, "error":null})
}
pub fn error(message: impl ToString) -> Value {
    json!({"schema":"warehouse-cosmic-ui/1", "ok":false, "data":null, "error":message.to_string()})
}
