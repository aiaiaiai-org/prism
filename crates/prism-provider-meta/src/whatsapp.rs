// © 2026 aiaiaiai · aiaiaiai.org
// SPDX-License-Identifier: Apache-2.0

//! WhatsApp direct-message and channel-post adapter.

use std::{collections::BTreeSet, fmt, sync::Arc};

use async_trait::async_trait;
use prism_core::{
    AltTextCapabilities, ChannelRef, CredentialRef, DeliveryError, DeliveryErrorClass, Extensions,
    MediaCapabilities, NamespacedKey, ProviderCapabilities, ProviderId, ProviderReceipt,
    PublicationFormat, TextCapabilities, ValidationIssue,
};
use prism_provider::{ProviderAdapter, ProviderPublishRequest, ProviderTargetContext};
use reqwest::{Client, Url, header::RETRY_AFTER};
use serde::Deserialize;
use serde_json::json;

use crate::{
    MetaAccessToken, MetaExternalId, MetaNumericId, MetaTransportConfigError, MetaTransportError,
    MetaTransportErrorKind, MetaValueError, classify_graph_error, validated_api_base,
};

/// Stable Prism provider identifier for WhatsApp.
pub const WHATSAPP_PROVIDER_ID: &str = "meta.whatsapp";
const DEFAULT_API_BASE: &str = "https://graph.facebook.com/v26.0/";

/// Canonical recipient identifier for one individual WhatsApp destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhatsAppRecipient(String);

impl WhatsAppRecipient {
    /// Creates a canonical digits-only recipient identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, MetaValueError> {
        let value = value.into();
        if !(5..=32).contains(&value.len())
            || !value.chars().all(|character| character.is_ascii_digit())
        {
            return Err(MetaValueError::InvalidRecipient);
        }
        Ok(Self(value))
    }

    /// Returns the provider recipient identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Resolved WhatsApp sender/recipient/token binding for one direct message.
pub struct WhatsAppBinding {
    phone_number_id: MetaNumericId,
    recipient: WhatsAppRecipient,
    access_token: MetaAccessToken,
}

impl WhatsAppBinding {
    /// Creates a configured outbound individual-message binding.
    #[must_use]
    pub const fn new(
        phone_number_id: MetaNumericId,
        recipient: WhatsAppRecipient,
        access_token: MetaAccessToken,
    ) -> Self {
        Self {
            phone_number_id,
            recipient,
            access_token,
        }
    }

    /// Returns the sender phone-number identifier.
    #[must_use]
    pub const fn phone_number_id(&self) -> &MetaNumericId {
        &self.phone_number_id
    }

    /// Returns the configured individual recipient.
    #[must_use]
    pub const fn recipient(&self) -> &WhatsAppRecipient {
        &self.recipient
    }
}

impl fmt::Debug for WhatsAppBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WhatsAppBinding")
            .field("phone_number_id", &self.phone_number_id)
            .field("recipient", &self.recipient)
            .field("access_token", &self.access_token)
            .finish()
    }
}

/// Semantic kind of a configured WhatsApp target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhatsAppDestinationKind {
    /// One recipient addressed through the Business Platform messages API.
    Individual,
    /// One WhatsApp Channel receiving a public one-way update.
    Channel,
}

/// Resolves the semantic destination represented by an opaque Prism channel reference.
#[async_trait]
pub trait WhatsAppDestinationResolver: Send + Sync + 'static {
    /// Resolves whether this target is an individual recipient or a WhatsApp Channel.
    async fn destination_kind(
        &self,
        channel: &ChannelRef,
    ) -> Result<WhatsAppDestinationKind, DeliveryError>;
}

/// Resolves opaque Prism channel/credential references to one direct-message destination.
#[async_trait]
pub trait WhatsAppBindingResolver: Send + Sync + 'static {
    /// Resolves sender, individual recipient, and credential atomically.
    async fn resolve(
        &self,
        channel: &ChannelRef,
        credential: &CredentialRef,
    ) -> Result<WhatsAppBinding, DeliveryError>;
}

/// WhatsApp Business Platform direct-message transport.
#[async_trait]
pub trait WhatsAppTransport: Send + Sync + 'static {
    /// Sends one individual text message.
    async fn send_text(
        &self,
        binding: &WhatsAppBinding,
        text: &str,
    ) -> Result<MetaExternalId, MetaTransportError>;
}

/// Injected WhatsApp Channel publishing boundary.
///
/// Meta's documented WhatsApp Cloud API does not currently expose a Channels
/// publishing endpoint. Implementations may bridge another controlled transport,
/// and can later be replaced by an official Meta transport without changing the
/// provider-neutral Prism contract.
#[async_trait]
pub trait WhatsAppChannelPublisher: Send + Sync + 'static {
    /// Validates that the configured channel and credential can be used without publishing.
    async fn validate_target(
        &self,
        channel: &ChannelRef,
        credential: &CredentialRef,
    ) -> Result<(), DeliveryError>;

    /// Publishes one text update to the configured WhatsApp Channel.
    ///
    /// Implementations must return a safe typed `DeliveryError` for pre-publication
    /// failures and must classify ambiguous final-action failures as `outcome_unknown`.
    async fn publish_text(
        &self,
        channel: &ChannelRef,
        credential: &CredentialRef,
        text: &str,
    ) -> Result<MetaExternalId, DeliveryError>;
}

/// Reqwest transport for the official WhatsApp Cloud API messages endpoint.
#[derive(Clone, Debug)]
pub struct ReqwestWhatsAppTransport {
    client: Client,
    api_base: Url,
}

impl ReqwestWhatsAppTransport {
    /// Creates a transport for the current production Graph API version.
    pub fn new(client: Client) -> Result<Self, MetaTransportConfigError> {
        Self::with_api_base(client, DEFAULT_API_BASE)
    }

    /// Creates a transport with an explicit HTTPS Graph API base.
    pub fn with_api_base(client: Client, api_base: &str) -> Result<Self, MetaTransportConfigError> {
        Ok(Self {
            client,
            api_base: validated_api_base(api_base)?,
        })
    }

    fn endpoint(&self, phone_number_id: &MetaNumericId) -> Result<Url, MetaTransportError> {
        let mut url = self.api_base.clone();
        let mut segments = url.path_segments_mut().map_err(|_| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                "whatsapp.transport.invalid_base_url",
            )
        })?;
        segments.pop_if_empty();
        segments.push(phone_number_id.as_str());
        segments.push("messages");
        drop(segments);
        Ok(url)
    }
}

#[async_trait]
impl WhatsAppTransport for ReqwestWhatsAppTransport {
    async fn send_text(
        &self,
        binding: &WhatsAppBinding,
        text: &str,
    ) -> Result<MetaExternalId, MetaTransportError> {
        let response = self
            .client
            .post(self.endpoint(binding.phone_number_id())?)
            .bearer_auth(binding.access_token.expose_secret())
            .json(&json!({
                "messaging_product": "whatsapp",
                "recipient_type": "individual",
                "to": binding.recipient().as_str(),
                "type": "text",
                "text": {
                    "preview_url": false,
                    "body": text
                }
            }))
            .send()
            .await
            .map_err(|_| {
                MetaTransportError::new(MetaTransportErrorKind::Transient, "whatsapp.transport")
            })?;
        parse_message_response(response).await
    }
}

struct WhatsAppChannelSupport {
    destinations: Arc<dyn WhatsAppDestinationResolver>,
    publisher: Arc<dyn WhatsAppChannelPublisher>,
}

/// WhatsApp adapter supporting direct messages and optional channel posts.
pub struct WhatsAppAdapter {
    provider_id: ProviderId,
    bindings: Arc<dyn WhatsAppBindingResolver>,
    transport: Arc<dyn WhatsAppTransport>,
    channel_support: Option<WhatsAppChannelSupport>,
}

impl WhatsAppAdapter {
    /// Creates the official direct-message adapter.
    ///
    /// Channel posting remains disabled until `with_channel_posts` supplies a
    /// destination resolver and channel publishing implementation.
    #[must_use]
    pub fn new(
        bindings: Arc<dyn WhatsAppBindingResolver>,
        transport: Arc<dyn WhatsAppTransport>,
    ) -> Self {
        Self {
            provider_id: ProviderId::new(WHATSAPP_PROVIDER_ID)
                .expect("the static WhatsApp provider ID is valid"),
            bindings,
            transport,
            channel_support: None,
        }
    }

    /// Enables target-specific WhatsApp Channel posting behind an injected boundary.
    #[must_use]
    pub fn with_channel_posts(
        mut self,
        destinations: Arc<dyn WhatsAppDestinationResolver>,
        publisher: Arc<dyn WhatsAppChannelPublisher>,
    ) -> Self {
        self.channel_support = Some(WhatsAppChannelSupport {
            destinations,
            publisher,
        });
        self
    }

    async fn destination_kind(
        &self,
        channel: &ChannelRef,
    ) -> Result<WhatsAppDestinationKind, DeliveryError> {
        match &self.channel_support {
            Some(support) => support.destinations.destination_kind(channel).await,
            None => Ok(WhatsAppDestinationKind::Individual),
        }
    }

    fn required_credential<'a>(
        &self,
        request: &'a ProviderPublishRequest,
    ) -> Result<&'a CredentialRef, DeliveryError> {
        request.target.credential.as_ref().ok_or_else(|| {
            DeliveryError::new(
                DeliveryErrorClass::AuthRequired,
                "whatsapp.credential.required",
                "WhatsApp delivery requires a configured credential reference",
            )
        })
    }

    async fn resolve_binding(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<WhatsAppBinding, DeliveryError> {
        let credential = self.required_credential(request)?;
        self.bindings
            .resolve(&request.target.channel, credential)
            .await
    }

    fn channel_support(&self) -> Result<&WhatsAppChannelSupport, DeliveryError> {
        self.channel_support.as_ref().ok_or_else(|| {
            DeliveryError::new(
                DeliveryErrorClass::InvalidRequest,
                "whatsapp.channel.unavailable",
                "WhatsApp Channel publishing is not configured for this adapter",
            )
        })
    }

    fn expected_format(kind: WhatsAppDestinationKind) -> PublicationFormat {
        match kind {
            WhatsAppDestinationKind::Individual => PublicationFormat::Message,
            WhatsAppDestinationKind::Channel => PublicationFormat::Post,
        }
    }

    fn ensure_format(
        request: &ProviderPublishRequest,
        kind: WhatsAppDestinationKind,
    ) -> Result<(), DeliveryError> {
        let expected = Self::expected_format(kind);
        if request.variant.format == expected {
            return Ok(());
        }
        Err(DeliveryError::new(
            DeliveryErrorClass::InvalidRequest,
            "whatsapp.format.unsupported_for_destination",
            match kind {
                WhatsAppDestinationKind::Individual => {
                    "WhatsApp individual recipients require message format"
                }
                WhatsAppDestinationKind::Channel => {
                    "WhatsApp Channels require post format"
                }
            },
        ))
    }

    fn text<'a>(request: &'a ProviderPublishRequest) -> Result<&'a str, DeliveryError> {
        request
            .variant
            .body
            .text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| {
                DeliveryError::new(
                    DeliveryErrorClass::InvalidRequest,
                    "whatsapp.text.required",
                    "the WhatsApp text adapter requires non-empty text",
                )
            })
    }
}

#[async_trait]
impl ProviderAdapter for WhatsAppAdapter {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    async fn capabilities(
        &self,
        target: &ProviderTargetContext,
    ) -> Result<ProviderCapabilities, DeliveryError> {
        let destination = self.destination_kind(&target.channel).await?;
        let mut extensions = Extensions::new();
        extensions.insert(
            NamespacedKey::new("meta.whatsapp.native_idempotency")
                .expect("static extension key is valid"),
            json!(false),
        );
        extensions.insert(
            NamespacedKey::new("meta.whatsapp.destination_type")
                .expect("static extension key is valid"),
            json!(match destination {
                WhatsAppDestinationKind::Individual => "individual",
                WhatsAppDestinationKind::Channel => "channel",
            }),
        );
        if destination == WhatsAppDestinationKind::Individual {
            extensions.insert(
                NamespacedKey::new("meta.whatsapp.recipient_type")
                    .expect("static extension key is valid"),
                json!("individual"),
            );
        }
        if destination == WhatsAppDestinationKind::Channel {
            extensions.insert(
                NamespacedKey::new("meta.whatsapp.channel_transport")
                    .expect("static extension key is valid"),
                json!("injected"),
            );
        }
        Ok(ProviderCapabilities {
            provider_id: self.provider_id.clone(),
            revision: Some(match destination {
                WhatsAppDestinationKind::Individual => "whatsapp-cloud.individual-text-v1",
                WhatsAppDestinationKind::Channel => "whatsapp-channel.text-v1",
            }
            .to_owned()),
            formats: BTreeSet::from([Self::expected_format(destination)]),
            text: TextCapabilities {
                supported: true,
                max_characters: None,
            },
            media: MediaCapabilities {
                supported_kinds: BTreeSet::new(),
                max_items: Some(0),
                mixed_kinds: false,
                alt_text: AltTextCapabilities {
                    supported: false,
                    max_characters: None,
                },
            },
            extensions,
        })
    }

    async fn validate_publish(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<Vec<ValidationIssue>, DeliveryError> {
        let destination = self.destination_kind(&request.target.channel).await?;
        let mut issues = Vec::new();
        if request.target.credential.is_none() {
            issues.push(ValidationIssue::error(
                "whatsapp.credential.required",
                format!("/targets/{}/credential", request.target.id),
                "WhatsApp delivery requires a configured credential reference",
            ));
        }
        for (key, _) in request.target.options.iter() {
            issues.push(ValidationIssue::error(
                "whatsapp.option.unsupported",
                format!("/targets/{}/options/{key}", request.target.id),
                "this WhatsApp adapter version does not implement provider options",
            ));
        }
        if request.variant.format != Self::expected_format(destination) {
            issues.push(ValidationIssue::error(
                "whatsapp.format.unsupported_for_destination",
                format!("/variants/{}/format", request.variant.id),
                match destination {
                    WhatsAppDestinationKind::Individual => {
                        "WhatsApp individual recipients require message format"
                    }
                    WhatsAppDestinationKind::Channel => "WhatsApp Channels require post format",
                },
            ));
        }
        if !issues.iter().any(ValidationIssue::is_error) {
            match destination {
                WhatsAppDestinationKind::Individual => {
                    self.resolve_binding(request).await?;
                }
                WhatsAppDestinationKind::Channel => {
                    let credential = self.required_credential(request)?;
                    self.channel_support()?
                        .publisher
                        .validate_target(&request.target.channel, credential)
                        .await?;
                }
            }
        }
        Ok(issues)
    }

    async fn publish(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<ProviderReceipt, DeliveryError> {
        let destination = self.destination_kind(&request.target.channel).await?;
        Self::ensure_format(request, destination)?;
        let text = Self::text(request)?;
        let id = match destination {
            WhatsAppDestinationKind::Individual => {
                let binding = self.resolve_binding(request).await?;
                self.transport
                    .send_text(&binding, text)
                    .await
                    .map_err(map_send_error)?
            }
            WhatsAppDestinationKind::Channel => {
                let credential = self.required_credential(request)?;
                self.channel_support()?
                    .publisher
                    .publish_text(&request.target.channel, credential, text)
                    .await?
            }
        };
        Ok(ProviderReceipt {
            external_id: id.as_str().to_owned(),
            external_url: None,
            details: Extensions::new(),
        })
    }
}

async fn parse_message_response(
    response: reqwest::Response,
) -> Result<MetaExternalId, MetaTransportError> {
    let status = response.status();
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let body = response.bytes().await.map_err(|_| {
        MetaTransportError::new(
            MetaTransportErrorKind::InvalidResponse,
            "whatsapp.response.unreadable",
        )
    })?;
    if status.is_success() {
        let response: SendResponse = serde_json::from_slice(&body).map_err(|_| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                "whatsapp.response.invalid_json",
            )
        })?;
        let id = response.messages.into_iter().next().ok_or_else(|| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                "whatsapp.response.missing_message_id",
            )
        })?;
        return MetaExternalId::new(id.id).map_err(|_| {
            MetaTransportError::new(
                MetaTransportErrorKind::InvalidResponse,
                "whatsapp.response.invalid_message_id",
            )
        });
    }
    Err(classify_graph_error("whatsapp", status, &body, retry_after))
}

#[derive(Deserialize)]
struct SendResponse {
    #[serde(default)]
    messages: Vec<MessageIdResponse>,
}

#[derive(Deserialize)]
struct MessageIdResponse {
    id: String,
}

fn map_send_error(error: MetaTransportError) -> DeliveryError {
    let class = match error.kind {
        MetaTransportErrorKind::Authentication => DeliveryErrorClass::AuthRequired,
        MetaTransportErrorKind::RateLimited => DeliveryErrorClass::RateLimited,
        MetaTransportErrorKind::Rejected => DeliveryErrorClass::ProviderRejected,
        MetaTransportErrorKind::Transient | MetaTransportErrorKind::InvalidResponse => {
            DeliveryErrorClass::OutcomeUnknown
        }
    };
    let mut mapped = DeliveryError::new(
        class,
        error.code,
        match class {
            DeliveryErrorClass::AuthRequired => {
                "WhatsApp authorization is missing, expired, or insufficient"
            }
            DeliveryErrorClass::RateLimited => "WhatsApp messaging is rate limited",
            DeliveryErrorClass::ProviderRejected => "WhatsApp rejected the message request",
            DeliveryErrorClass::OutcomeUnknown => {
                "WhatsApp message outcome is unknown; reconcile provider state before retrying"
            }
            DeliveryErrorClass::InvalidRequest
            | DeliveryErrorClass::Retryable
            | DeliveryErrorClass::Terminal => "WhatsApp messaging failed",
        },
    );
    if let Some(seconds) = error.retry_after_seconds {
        mapped = mapped.with_retry_after(seconds);
    }
    mapped
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use prism_core::{
        ContentBody, ContentVariant, IdempotencyKey, IdempotencyScope, LocaleTag, Provenance,
        ProvenanceKind, PublishTarget, RequestId, TargetId, VariantId, VariantSelection,
    };

    use super::*;

    struct StaticBindingResolver;

    #[async_trait]
    impl WhatsAppBindingResolver for StaticBindingResolver {
        async fn resolve(
            &self,
            _channel: &ChannelRef,
            _credential: &CredentialRef,
        ) -> Result<WhatsAppBinding, DeliveryError> {
            Ok(WhatsAppBinding::new(
                MetaNumericId::new("123456789").expect("valid phone-number ID"),
                WhatsAppRecipient::new("380501234567").expect("valid recipient"),
                MetaAccessToken::new("top-secret-token").expect("valid token"),
            ))
        }
    }

    struct StaticDirectTransport;

    #[async_trait]
    impl WhatsAppTransport for StaticDirectTransport {
        async fn send_text(
            &self,
            _binding: &WhatsAppBinding,
            _text: &str,
        ) -> Result<MetaExternalId, MetaTransportError> {
            MetaExternalId::new("wamid.direct").map_err(|_| {
                MetaTransportError::new(
                    MetaTransportErrorKind::InvalidResponse,
                    "whatsapp.test.invalid_id",
                )
            })
        }
    }

    struct StaticDestinationResolver(WhatsAppDestinationKind);

    #[async_trait]
    impl WhatsAppDestinationResolver for StaticDestinationResolver {
        async fn destination_kind(
            &self,
            _channel: &ChannelRef,
        ) -> Result<WhatsAppDestinationKind, DeliveryError> {
            Ok(self.0)
        }
    }

    struct RecordingChannelPublisher {
        published_text: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl WhatsAppChannelPublisher for RecordingChannelPublisher {
        async fn validate_target(
            &self,
            _channel: &ChannelRef,
            _credential: &CredentialRef,
        ) -> Result<(), DeliveryError> {
            Ok(())
        }

        async fn publish_text(
            &self,
            _channel: &ChannelRef,
            _credential: &CredentialRef,
            text: &str,
        ) -> Result<MetaExternalId, DeliveryError> {
            self.published_text
                .lock()
                .expect("test publisher lock")
                .push(text.to_owned());
            MetaExternalId::new("channel-update-1").map_err(|_| {
                DeliveryError::new(
                    DeliveryErrorClass::Terminal,
                    "whatsapp.channel.test.invalid_id",
                    "invalid test channel update identifier",
                )
            })
        }
    }

    fn adapter(
        destination: WhatsAppDestinationKind,
    ) -> (WhatsAppAdapter, Arc<RecordingChannelPublisher>) {
        let publisher = Arc::new(RecordingChannelPublisher {
            published_text: Mutex::new(Vec::new()),
        });
        let adapter = WhatsAppAdapter::new(
            Arc::new(StaticBindingResolver),
            Arc::new(StaticDirectTransport),
        )
        .with_channel_posts(
            Arc::new(StaticDestinationResolver(destination)),
            publisher.clone(),
        );
        (adapter, publisher)
    }

    fn request(format: PublicationFormat) -> ProviderPublishRequest {
        ProviderPublishRequest {
            request_id: RequestId::new("request-1").expect("valid request ID"),
            idempotency: IdempotencyScope {
                root: IdempotencyKey::new("publication-1").expect("valid idempotency key"),
                target_id: TargetId::new("whatsapp-target").expect("valid target ID"),
            },
            target: PublishTarget {
                id: TargetId::new("whatsapp-target").expect("valid target ID"),
                provider_id: ProviderId::new(WHATSAPP_PROVIDER_ID).expect("valid provider ID"),
                channel: ChannelRef::new("whatsapp.destination").expect("valid channel"),
                credential: Some(
                    CredentialRef::new("whatsapp.destination").expect("valid credential ref"),
                ),
                selection: VariantSelection::Exact {
                    variant_id: VariantId::new("uk").expect("valid variant ID"),
                },
                options: Extensions::new(),
            },
            variant: ContentVariant {
                id: VariantId::new("uk").expect("valid variant ID"),
                locale: LocaleTag::new("uk-UA").expect("valid locale"),
                voice_profile: None,
                audience: None,
                provider_target: Some(
                    ProviderId::new(WHATSAPP_PROVIDER_ID).expect("valid provider ID"),
                ),
                format,
                body: ContentBody {
                    text: Some("привіт, WhatsApp Channel".to_owned()),
                    media: Vec::new(),
                },
                provenance: Provenance {
                    kind: ProvenanceKind::Human,
                    producer: None,
                    source_refs: Vec::new(),
                },
                extensions: Extensions::new(),
            },
        }
    }

    #[test]
    fn recipient_is_canonical_digits_only() {
        assert!(WhatsAppRecipient::new("380501234567").is_ok());
        assert_eq!(
            WhatsAppRecipient::new("+380501234567").expect_err("plus sign is not canonical"),
            MetaValueError::InvalidRecipient
        );
    }

    #[test]
    fn transport_rejects_plaintext_graph_bases() {
        let error = ReqwestWhatsAppTransport::with_api_base(
            Client::new(),
            "http://graph.facebook.com/v26.0/",
        )
        .expect_err("HTTP must be rejected");
        assert_eq!(error, MetaTransportConfigError::InsecureBaseUrl);
    }

    #[tokio::test]
    async fn direct_target_advertises_message_only() {
        let (adapter, _) = adapter(WhatsAppDestinationKind::Individual);
        let capabilities = adapter
            .capabilities(&ProviderTargetContext::from(&request(PublicationFormat::Message).target))
            .await
            .expect("capabilities");

        assert_eq!(capabilities.formats, BTreeSet::from([PublicationFormat::Message]));
        assert!(!capabilities.formats.contains(&PublicationFormat::Post));
    }

    #[tokio::test]
    async fn channel_target_advertises_post_only() {
        let (adapter, _) = adapter(WhatsAppDestinationKind::Channel);
        let capabilities = adapter
            .capabilities(&ProviderTargetContext::from(&request(PublicationFormat::Post).target))
            .await
            .expect("capabilities");

        assert_eq!(capabilities.formats, BTreeSet::from([PublicationFormat::Post]));
        assert!(!capabilities.formats.contains(&PublicationFormat::Message));
    }

    #[tokio::test]
    async fn channel_post_uses_injected_publisher() {
        let (adapter, publisher) = adapter(WhatsAppDestinationKind::Channel);
        let receipt = adapter
            .publish(&request(PublicationFormat::Post))
            .await
            .expect("channel publish succeeds");

        assert_eq!(receipt.external_id, "channel-update-1");
        assert_eq!(
            publisher
                .published_text
                .lock()
                .expect("test publisher lock")
                .as_slice(),
            ["привіт, WhatsApp Channel"]
        );
    }

    #[tokio::test]
    async fn channel_rejects_message_format() {
        let (adapter, _) = adapter(WhatsAppDestinationKind::Channel);
        let issues = adapter
            .validate_publish(&request(PublicationFormat::Message))
            .await
            .expect("validation succeeds");

        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "whatsapp.format.unsupported_for_destination")
        );
    }
}
