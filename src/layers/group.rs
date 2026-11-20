//! A layer group: a named container with its own visibility and opacity.
//!
//! Groups hold paint layers (not other groups, for now). Their properties are
//! inherited by every layer inside them when the stack composites.

use super::layer::LayerId;

/// Everything about a group except its membership.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupProps {
    pub name: String,
    pub visible: bool,
    /// `0.0..=1.0`, multiplied into each child layer's opacity.
    pub opacity: f32,
}

/// A group of layers.
#[derive(Clone, Debug)]
pub struct Group {
    id: LayerId,
    props: GroupProps,
    /// Collapsed groups hide their children in the layer list. This is view
    /// state, so it is deliberately not part of [`GroupProps`] and never enters
    /// the undo history.
    collapsed: bool,
}

impl Group {
    pub fn new(id: LayerId, name: impl Into<String>) -> Self {
        Self {
            id,
            props: GroupProps {
                name: name.into(),
                visible: true,
                opacity: 1.0,
            },
            collapsed: false,
        }
    }

    pub fn id(&self) -> LayerId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.props.name
    }

    pub fn visible(&self) -> bool {
        self.props.visible
    }

    pub fn opacity(&self) -> f32 {
        self.props.opacity
    }

    pub fn props(&self) -> GroupProps {
        self.props.clone()
    }

    /// Replace the properties; opacity is clamped, empty names are ignored.
    pub fn set_props(&mut self, mut props: GroupProps) {
        props.opacity = if props.opacity.is_finite() {
            props.opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if props.name.trim().is_empty() {
            props.name = self.props.name.clone();
        }
        self.props = props;
    }

    pub fn collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }
}
