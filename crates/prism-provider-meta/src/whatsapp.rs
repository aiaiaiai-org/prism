// © 2026 aiaiaiai · aiaiaiai.org
// SPDX-License-Identifier: Apache-2.0

//! WhatsApp Business Platform individual text-message adapter.

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

/// Stable Prism provider identifier for WhatsApp Business Platform.
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

/// Resolved WhatsApp sender/recipient/token binding.
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

/// Resolves opaque Prism channel/credential references to one WhatsApp destination.
#[async_trait]
pub trait WhatsAppBindingResolver: Send + Sync + 'static {
    /// Resolves sender, individual recipient, and credential atomically.
    async fn resolve(
        &self,
        channel: &ChannelRef,
        credential: &CredentialRef,
    ) -> Result<WhatsAppBinding, DeliveryError>;
}

/// WhatsApp Business Platform text-message transport.
#[async_trait]
pub trait WhatsAppTransport: Send + Sync + 'static {
    /// Sends one individual text message.
    async fn send_text(
        &self,
        binding: &WhatsAppBinding,
        text: &str,
    ) -> Result<MetaExternalId, MetaTransportError>;
}

/// Reqwest transport for the WhatsApp Cloud API messages endpoint.
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

/// Official WhatsApp Business individual text-message adapter.
pub struct WhatsAppAdapter {
    provider_id: ProviderId,
    bindings: Arc<dyn WhatsAppBindingResolver>,
    transport: Arc<dyn WhatsAppTransport>,
}

impl WhatsAppAdapter {
    /// Creates an adapter with injected binding and transport boundaries.
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
        }
    }

    async fn resolve_binding(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<WhatsAppBinding, DeliveryError> {
        let credential = request.target.credential.as_ref().ok_or_else(|| {
            DeliveryError::new(
                DeliveryErrorClass::AuthRequired,
                "whatsapp.credential.required",
                "WhatsApp messaging requires a configured credential reference",
            )
        })?;
        self.bindings
            .resolve(&request.target.channel, credential)
            .await
    }
}

#[async_trait]
impl ProviderAdapter for WhatsAppAdapter {
    fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    async fn capabilities(
        &self,
        _target: &ProviderTargetContext,
    ) -> Result<ProviderCapabilities, DeliveryError> {
        let mut extensions = Extensions::new();
        extensions.insert(
            NamespacedKey::new("meta.whatsapp.native_idempotency")
                .expect("static extension key is valid"),
            json!(false),
        );
        extensions.insert(
            NamespacedKey::new("meta.whatsapp.recipient_type")
                .expect("static extension key is valid"),
            json!("individual"),
        );
        Ok(ProviderCapabilities {
            provider_id: self.provider_id.clone(),
            revision: Some("whatsapp-cloud.individual-text-v1".to_owned()),
            formats: BTreeSet::from([PublicationFormat::Message]),
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
        let mut issues = Vec::new();
        if request.target.credential.is_none() {
            issues.push(ValidationIssue::error(
                "whatsapp.credential.required",
                format!("/targets/{}/credential", request.target.id),
                "WhatsApp messaging requires a configured credential reference",
            ));
        }
        for (key, _) in request.target.options.iter() {
            issues.push(ValidationIssue::error(
                "whatsapp.option.unsupported",
                format!("/targets/{}/options/{key}", request.target.id),
                "this WhatsApp adapter version does not implement provider options",
            ));
        }
        if !issues.iter().any(ValidationIssue::is_error) {
            self.resolve_binding(request).await?;
        }
        Ok(issues)
    }

    async fn publish(
        &self,
        request: &ProviderPublishRequest,
    ) -> Result<ProviderReceipt, DeliveryError> {
        let binding = self.resolve_binding(request).await?;
        let text = request
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
            })?;
        let id = self
            .transport
            .send_text(&binding, text)
            .await
            .map_err(map_send_error)?;
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
    use super::*;

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
}
