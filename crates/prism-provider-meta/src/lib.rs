// © 2026 aiaiaiai · aiaiaiai.org
// SPDX-License-Identifier: Apache-2.0

//! Meta-family Prism adapters with separate provider semantics and shared private primitives.
//!
//! OAuth lifecycle, token persistence, account ownership, and channel configuration
//! remain outside this crate. Callers inject resolvers that bind Prism's opaque
//! channel and credential references at the provider boundary.

mod common;
pub mod facebook;
pub mod instagram;
pub mod whatsapp;

pub(crate) use common::{classify_graph_error, validated_api_base};
pub use common::{
    MetaAccessToken, MetaExternalId, MetaNumericId, MetaTransportConfigError, MetaTransportError,
    MetaTransportErrorKind, MetaValueError, PublicMediaUrl,
};

pub use facebook::{
    FACEBOOK_PROVIDER_ID, FacebookAdapter, FacebookBinding, FacebookBindingResolver,
    FacebookTransport, ReqwestFacebookTransport,
};
pub use instagram::{
    INSTAGRAM_PROVIDER_ID, InstagramAdapter, InstagramBinding, InstagramBindingResolver,
    InstagramMediaResolver, InstagramTransport, ReqwestInstagramTransport,
};
pub use whatsapp::{
    WHATSAPP_PROVIDER_ID, ReqwestWhatsAppTransport, WhatsAppAdapter, WhatsAppBinding,
    WhatsAppBindingResolver, WhatsAppRecipient, WhatsAppTransport,
};
