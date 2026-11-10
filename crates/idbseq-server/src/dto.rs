//! Request and response bodies (`docs/API.md`), with `OpenAPI` schemas.

use std::num::{NonZeroU32, NonZeroU64};

use idbseq_core::collection::{
    Collection, CollectionPatch, CollectionSpec, Durability, Id, IssuedRange, ResetOutcome,
};
use idbseq_core::validation::ValidationErrors;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::auth::keys::{KeyRecord, Scope};
use crate::clock::rfc3339;

/// How a collection makes issued IDs durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum DurabilityDto {
    /// Every ID is on disk before it is returned.
    Strict,
    /// `size` IDs are reserved on disk at a time; a crash leaves a gap, never a duplicate.
    Block { size: u32 },
}

impl From<Durability> for DurabilityDto {
    fn from(d: Durability) -> Self {
        match d {
            Durability::Strict => Self::Strict,
            Durability::Block { size } => Self::Block { size: size.get() },
        }
    }
}

impl DurabilityDto {
    fn into_core(self) -> Result<Durability, ValidationErrors> {
        match self {
            Self::Strict => Ok(Durability::Strict),
            Self::Block { size } => NonZeroU32::new(size)
                .map(|size| Durability::Block { size })
                .ok_or_else(|| {
                    ValidationErrors::field("durability", "block size must be greater than 0")
                }),
        }
    }
}

/// A collection as returned by the API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CollectionDto {
    pub name: String,
    pub start: Id,
    pub step: u64,
    pub max: Id,
    pub protected: bool,
    pub durability: DurabilityDto,
    /// RFC 3339, UTC.
    pub created_at: String,
    pub last_id: Option<Id>,
    pub next_id: Option<Id>,
}

impl From<&Collection> for CollectionDto {
    fn from(c: &Collection) -> Self {
        let spec = c.spec();
        Self {
            name: spec.name.clone(),
            start: spec.start,
            step: spec.step.get(),
            max: spec.max,
            protected: spec.protected,
            durability: spec.durability.into(),
            created_at: rfc3339(c.created_at()),
            last_id: c.last_id(),
            next_id: c.next_id(),
        }
    }
}

fn step_of(step: Option<u64>) -> Result<Option<NonZeroU64>, ValidationErrors> {
    step.map(|s| {
        NonZeroU64::new(s).ok_or_else(|| ValidationErrors::field("step", "must be greater than 0"))
    })
    .transpose()
}

/// `POST /v1/collections`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateCollectionBody {
    pub name: String,
    pub start: Option<Id>,
    pub step: Option<u64>,
    pub max: Option<Id>,
    pub protected: Option<bool>,
    pub durability: Option<DurabilityDto>,
}

impl CreateCollectionBody {
    /// The spec with documented defaults.
    ///
    /// # Errors
    ///
    /// `step` or block size of 0.
    pub fn into_spec(self) -> Result<CollectionSpec, ValidationErrors> {
        let defaults = CollectionSpec::named(self.name);
        Ok(CollectionSpec {
            start: self.start.unwrap_or(defaults.start),
            step: step_of(self.step)?.unwrap_or(defaults.step),
            max: self.max.unwrap_or(defaults.max),
            protected: self.protected.unwrap_or(defaults.protected),
            durability: self
                .durability
                .map(DurabilityDto::into_core)
                .transpose()?
                .unwrap_or_default(),
            ..defaults
        })
    }
}

/// `PATCH /v1/collections/{name}`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchCollectionBody {
    pub start: Option<Id>,
    pub step: Option<u64>,
    pub max: Option<Id>,
    pub protected: Option<bool>,
    pub durability: Option<DurabilityDto>,
}

impl PatchCollectionBody {
    /// The core patch.
    ///
    /// # Errors
    ///
    /// `step` or block size of 0.
    pub fn into_patch(self) -> Result<CollectionPatch, ValidationErrors> {
        Ok(CollectionPatch {
            start: self.start,
            step: step_of(self.step)?,
            max: self.max,
            protected: self.protected,
            durability: self.durability.map(DurabilityDto::into_core).transpose()?,
        })
    }
}

/// Body of `DELETE /v1/collections/{name}` (or use `?confirm=`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct ConfirmBody {
    /// Must equal the collection name.
    pub confirm: Option<String>,
}

/// `POST /v1/collections/{name}/reset`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResetBody {
    /// Must equal the collection name.
    pub confirm: Option<String>,
    /// The next ID to issue; default `start`.
    pub value: Option<Id>,
    /// Required to move the counter backwards.
    #[serde(default)]
    pub allow_rewind: bool,
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct NextQuery {
    /// Number of IDs (1–10,000 by default). Without it, the response is `{ "id" }`.
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1–500, default 50.
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IdDto {
    pub id: Id,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BatchDto {
    pub first: Id,
    pub last: Id,
    pub count: u32,
    pub step: u64,
}

impl From<IssuedRange> for BatchDto {
    fn from(r: IssuedRange) -> Self {
        Self {
            first: r.first,
            last: r.last,
            count: r.count.get(),
            step: r.step.get(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LastDto {
    pub last_id: Option<Id>,
    pub next_id: Option<Id>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ResetDto {
    pub previous_last_id: Option<Id>,
    pub last_id: Option<Id>,
    pub next_id: Option<Id>,
}

impl From<ResetOutcome> for ResetDto {
    fn from(o: ResetOutcome) -> Self {
        Self {
            previous_last_id: o.previous_last_id,
            last_id: o.last_id,
            next_id: o.next_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthDto {
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionDto {
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PasswordBody {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct KeyDto {
    pub id: String,
    pub name: String,
    pub scopes: Vec<Scope>,
    pub collections: Vec<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

impl From<&KeyRecord> for KeyDto {
    fn from(k: &KeyRecord) -> Self {
        Self {
            id: k.id.clone(),
            name: k.name.clone(),
            scopes: k.scopes.clone(),
            collections: k.collections.clone(),
            created_at: rfc3339(k.created_at),
            last_used_at: k.last_used_at.map(rfc3339),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatedKeyDto {
    #[serde(flatten)]
    pub key: KeyDto,
    /// The full key `idb_<id>_<secret>`. Shown only once.
    #[serde(rename = "key")]
    pub secret: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateKeyBody {
    pub name: String,
    pub scopes: Vec<Scope>,
    #[serde(default)]
    pub collections: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQuery {
    pub limit: Option<usize>,
    pub cursor: Option<u64>,
    pub collection: Option<String>,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuditDto {
    pub id: u64,
    pub at: String,
    pub actor: String,
    pub action: crate::audit::Action,
    pub collection: Option<String>,
    pub previous: Option<serde_json::Value>,
    pub new: Option<serde_json::Value>,
    pub request_id: Option<String>,
    pub client_ip: Option<String>,
}

impl From<crate::audit::Entry> for AuditDto {
    fn from(e: crate::audit::Entry) -> Self {
        Self {
            id: e.id,
            at: rfc3339(e.at),
            actor: e.event.actor,
            action: e.event.action,
            collection: e.event.collection,
            previous: e.event.previous,
            new: e.event.new,
            request_id: e.event.request_id,
            client_ip: e.event.client_ip,
        }
    }
}
