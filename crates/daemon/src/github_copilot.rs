//! Control-plane access to the GitHub Copilot provider for the single
//! `flowspace3` binary. The CLI already depends on this composition-root crate;
//! keeping provider access here preserves the enforced crate graph.

pub use fs3_providers::{
    CredentialSource, DeviceCode, GitHubCopilotCredential, GitHubCopilotModel,
    GitHubCopilotModelList, LoginState, TOKEN_ENV, finish_device_login, list_models,
    start_device_login,
};
