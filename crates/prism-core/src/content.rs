// © 2026 aiaiaiai · aiaiaiai.org
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{LocaleTag, MediaRef, NamespacedKey, ProviderId, VariantId, VoiceProfileRef};

/// Deterministically ordered, namespaced extension values.
#[derive(Clone, Debug, Default, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Extensions(BTreeMap<NamespacedKey, Value>);

impl Extensions {
    /// Creates an empty extension map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a value and returns the previous value, if any.
    pub fn insert(&mut self, key: NamespacedKey, value: Value) -> Option<Value> {
        self.0.insert(key, value)
    }

    /// Returns an iterator in stable key order.
    pub fn iter(&self) -> impl Iterator<Item = (&NamespacedKey, &Value)> {
        self.0.iter()
    }

    /// Returns whether the map has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<(NamespacedKey, Value)> for Extensions {
    fn from_iter<T: IntoIterator<Item = (NamespacedKey, Value)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Optional audience metadata, independent from locale and voice.
#[derive(Clone, Debug, Default, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audience {
    /// ISO 3166-1 alpha-2 country code when geographic targeting matters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Human-readable region or subdivision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Human-readable city.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// Product-owned audience segment reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment: Option<String>,
}

/// Origin of an explicit content variant.
#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    /// Written or approved directly by a person.
    Human,
    /// Imported without semantic transformation.
    Import,
    /// Produced by a deterministic transformation.
    DeterministicTransform,
    /// Produced by an AI system and made explicit before delivery.
    Ai,
}

/// Provenance metadata carried with a variant.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// How the variant was produced.
    pub kind: ProvenanceKind,
    /// Optional producer reference, such as a user or model profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    /// Opaque references to source facts or content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<String>,
}

/// Provider-neutral media kind.
#[derive(
    Clone, Copy, Debug, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    /// Still image.
    Image,
    /// Video asset.
    Video,
    /// Audio asset.
    Audio,
    /// Document or other downloadable file.
    Document,
}

/// Media reference and metadata; the bytes are resolved outside `prism-core`.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Media {
    /// Opaque media reference.
    #[serde(rename = "ref")]
    pub reference: MediaRef,
    /// Semantic media kind.
    pub kind: MediaKind,
    /// Optional trusted MIME type hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Accessibility text that must not be silently discarded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
    /// Namespaced media metadata.
    #[serde(default, skip_serializing_if = "Extensions::is_empty")]
    pub extensions: Extensions,
}

/// Shared publication surface. Provider-only surfaces use namespaced extensions
/// until they prove portable.
#[derive(
    Clone, Copy, Debug, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PublicationFormat {
    /// Standard feed or channel post.
    Post,
    /// Recipient-addressed message.
    Message,
    /// Ephemeral story-like surface.
    Story,
    /// Short-form video surface.
    ShortVideo,
    /// Native poll surface.
    Poll,
}

/// Text and media delivered as one variant.
#[derive(Clone, Debug, Default, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentBody {
    /// Explicit text. `None` differs from an invalid empty string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Ordered media items.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<Media>,
}

impl ContentBody {
    /// Returns whether the body has neither meaningful text nor media.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.as_ref().is_none_or(|text| text.trim().is_empty()) && self.media.is_empty()
    }

    /// Counts Unicode scalar values rather than UTF-8 bytes.
    #[must_use]
    pub fn text_character_count(&self) -> usize {
        self.text.as_ref().map_or(0, |text| text.chars().count())
    }
}

/// Explicit delivery candidate across independent localization dimensions.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentVariant {
    /// Request-local identifier.
    pub id: VariantId,
    /// BCP-47 content locale.
    pub locale: LocaleTag,
    /// Optional voice/style profile, independent from locale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_profile: Option<VoiceProfileRef>,
    /// Optional audience metadata, independent from locale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<Audience>,
    /// Optional provider restriction for a provider-native variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_target: Option<ProviderId>,
    /// Publication surface.
    pub format: PublicationFormat,
    /// Explicit content.
    pub body: ContentBody,
    /// Variant origin.
    pub provenance: Provenance,
    /// Namespaced content metadata.
    #[serde(default, skip_serializing_if = "Extensions::is_empty")]
    pub extensions: Extensions,
}

impl ContentVariant {
    /// Returns whether the variant is eligible for a provider target.
    #[must_use]
    pub fn supports_provider(&self, provider: &ProviderId) -> bool {
        self.provider_target
            .as_ref()
            .is_none_or(|target| target == provider)
    }
}
