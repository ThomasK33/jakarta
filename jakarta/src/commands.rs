use async_trait::async_trait;

#[async_trait]
pub trait JakartaCommand {
    async fn process(
        &mut self,
        command: String,
        path: String,
        field: Option<String>,
        default_value: Option<String>,
    ) -> String;
}
