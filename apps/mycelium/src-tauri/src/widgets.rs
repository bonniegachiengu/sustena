//! **A widget, in publishable form.**
//!
//! ★★ The same shape `AuthoredDefinition` already established for a `Σ`: a
//! serialisable host DTO that converts into the core type, with the **core's
//! own gate** deciding whether it may be used. `WidgetDecl` does not derive
//! serde (nothing in the core needed it to), and adding derives to the core
//! for a host concern is the wrong direction — the host owns its DTOs, per
//! ADR-0001 and `dto.rs`'s own header.
//!
//! ★★★ The conversion is deliberately **lossy in one direction only**: an
//! `AuthoredWidget` can always become a `WidgetDecl`, and whether that decl is
//! *usable* is `WidgetSet::load`'s answer, never this file's.

use serde::{Deserialize, Serialize};
use specta::Type;
use sustena_core::curated::BindingKey;
use sustena_core::widget::WidgetDecl;

/// A curated-UI card as authored or published.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthoredWidget {
    pub id: String,
    /// The render key a surface switches on.
    pub render: String,
    /// State paths it reads. Checked against the target's `dim(S)` at load.
    pub inputs: Vec<String>,
    /// Operators it may emit. Checked against the target's `T` at load.
    pub emits: Vec<String>,
    /// The event class that makes it eligible. ★ `None` is **Unit** — always
    /// eligible — which is a real binding rather than a missing one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_class: Option<String>,
}

impl AuthoredWidget {
    pub fn to_decl(&self) -> WidgetDecl {
        WidgetDecl {
            id: self.id.clone(),
            inputs: self.inputs.clone(),
            render: self.render.clone(),
            emits: self.emits.clone(),
            binding: match &self.event_class {
                Some(class) => BindingKey::event(class),
                None => BindingKey::Unit,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_event_class_is_unit_not_missing() {
        let w = AuthoredWidget {
            id: "card".into(),
            render: "readout".into(),
            inputs: vec!["balance".into()],
            emits: vec![],
            event_class: None,
        };
        assert_eq!(w.to_decl().binding, BindingKey::Unit);
    }

    #[test]
    fn an_event_class_survives_the_round_trip() {
        let w = AuthoredWidget {
            id: "spend".into(),
            render: "readout".into(),
            inputs: vec!["balance".into()],
            emits: vec!["budget.spend".into()],
            event_class: Some("event.finances.pocket_spent".into()),
        };
        let decl = w.to_decl();
        assert_eq!(decl.binding, BindingKey::event("event.finances.pocket_spent"));
        assert_eq!(decl.emits, vec!["budget.spend".to_string()]);

        // And it survives JSON, which is how it is published.
        let json = serde_json::to_value(&w).expect("serialises");
        let back: AuthoredWidget = serde_json::from_value(json).expect("round trips");
        assert_eq!(back, w);
    }
}
