# Provider implementation notes

Load this reference only when changing model transport. API versions are pinned by Cargo.lock; re-check official source before an SDK migration.

### Provider abstraction

`LlmClients` holds two `Box<dyn DualProvider>` trait objects (`DualProvider: DescriptionProvider + ScoringProvider`). The `api_key_provider!` macro generates a struct + constructor for each API-key provider (Claude, OpenAI, Gemini, DeepSeek) and delegates the two trait impls to the shared `provider_impls!` macro. Ollama defines its struct + constructor manually (different builder) and reuses `provider_impls!`. The generated struct stores a pre-built `CompletionModel` (which holds the provider client) plus the preamble string, and exposes a `new(api_key, model, preamble) -> Result<Self>` constructor; call sites in `build_provider()` delegate to it. A single `build_provider()` function matches on provider name strings and returns the boxed trait object. Adding a new provider means: add a struct (or macro invocation), implement both traits, add a match arm in `build_provider()`. DeepSeek is wired up in `build_provider()` but intentionally not in `default_providers()` because DeepSeek has no public vision model — users who want it must add a `[providers.deepseek]` section manually.

### Rig crate notes (rig-core 0.42)

- rig-core 0.41 split the `Agent` abstraction into a separate `rig-agent` crate. imgcull does not use it: one-shot preamble + prompt requests go directly through `CompletionModel` — `client.completion_model(name)`, then `model.completion_request(msg).preamble(preamble).send().await`.
- All providers: `Client::new(key)?` returns `Result<Client>` — must propagate with `?`
- Anthropic: `rig::providers::anthropic::Client::new(&key)?`
- OpenAI/Gemini/DeepSeek: `Client::new(&key)?`
- Ollama: `Client::builder().api_key(Nothing).base_url(&url).build()?` (no API key)
- `completion_model()` is on the `CompletionClient` trait — must be imported: `use rig::client::CompletionClient`
- Image messages: `UserContent::image_base64(data, Some(ImageMediaType::JPEG), None)` (3 args)
- User messages: `Message::User { content: Vec<UserContent> }` — `OneOrMany` is gone
- Response text: `CompletionResponse::choice` is `Vec<AssistantContent>`; filter for `AssistantContent::Text` (reasoning models may emit other block types first)
- Import paths: `rig::client::{CompletionClient, Nothing}`, `rig::completion::message::{AssistantContent, ImageMediaType, UserContent}`, `rig::completion::{CompletionModel, Message}`
