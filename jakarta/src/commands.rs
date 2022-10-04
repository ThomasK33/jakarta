use async_trait::async_trait;

#[async_trait]
pub trait JakartaCommand {
    async fn process(
        &mut self,
        command: String,
        args: String,
        default_value: Option<String>,
    ) -> String;
}
