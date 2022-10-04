use async_trait::async_trait;

pub struct EnvCommand {}

#[async_trait]
impl jakarta::JakartaCommand for EnvCommand {
    async fn process(&mut self, _: String, args: String, default_value: Option<String>) -> String {
        std::env::var(args.clone()).unwrap_or_else(|_| {
            tracing::warn!("Could not get environment variable {args}, resolving to default value");

            default_value.unwrap_or_else(|| "".to_owned())
        })
    }
}

#[cfg(test)]
mod tests {
    use jakarta::{Jakarta, JakartaCommand};
    use std::{collections::HashMap, sync::Arc};

    use tokio::sync::Mutex;

    use super::*;

    #[tokio::test]
    async fn it_interpolates_env_variables() {
        let mut commands: HashMap<&str, Arc<Mutex<dyn JakartaCommand>>> = HashMap::new();

        let env_cmd = Arc::new(Mutex::new(EnvCommand {}));
        commands.insert("env", env_cmd.clone());
        let jakarta = Jakarta::new(commands).unwrap();

        let result = jakarta
            .interpolate_string("asd ${env:UNKNOWN_VAR}".to_owned())
            .await;

        assert_eq!(result, "asd ".to_owned());

        std::env::set_var("VAR_KEY", "VAR_VALUE");
        let result = jakarta
            .interpolate_string("asd ${env:VAR_KEY}".to_owned())
            .await;

        assert_eq!(result, "asd VAR_VALUE".to_owned());

        let result = jakarta
            .interpolate_string("asd ${env:UNSET_KEY:-default_value}".to_owned())
            .await;

        assert_eq!(result, "asd default_value".to_owned());
    }

    #[tokio::test]
    async fn it_interpolates_constructed_env_vars() {
        let mut commands: HashMap<&str, Arc<Mutex<dyn JakartaCommand>>> = HashMap::new();

        let env_cmd = Arc::new(Mutex::new(EnvCommand {}));
        commands.insert("env", env_cmd.clone());
        let jakarta = Jakarta::new(commands).unwrap();

        std::env::set_var("VAR_1", "2");
        std::env::set_var("VAR_2", "VAR_VALUE");
        let result = jakarta
            .interpolate_string("asd ${env:VAR_${env:VAR_1}}".to_owned())
            .await;

        assert_eq!(result, "asd VAR_VALUE".to_owned());
    }

    #[tokio::test]
    async fn it_interpolates_constructed_env_vars_from_default_value() {
        let mut commands: HashMap<&str, Arc<Mutex<dyn JakartaCommand>>> = HashMap::new();

        let env_cmd = Arc::new(Mutex::new(EnvCommand {}));
        commands.insert("env", env_cmd.clone());
        let jakarta = Jakarta::new(commands).unwrap();

        std::env::set_var("VAR_2", "VAR_VALUE");
        let result = jakarta
            .interpolate_string("asd ${env:VAR_${env:VAR_3:-2}}".to_owned())
            .await;

        assert_eq!(result, "asd VAR_VALUE".to_owned());
    }
}
