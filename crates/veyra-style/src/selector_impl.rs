//! The Veyra [`SelectorImpl`]: the type bindings that adapt the
//! `selectors` crate to Veyra's naming and pseudo-class model.
//!
//! Every associated type is a Veyra-owned newtype (`docs/architecture/
//! dependency-graph.md` invariant 5): the upstream crate never chooses
//! our string representations. The types are plain `Box<str>` wrappers
//! — selector identifiers are compared by exact string equality at
//! match time, and `PrecomputedHash` supplies the crate's bloom-filter
//! and map-hash fast paths.
//!
//! Pseudo-element and pseudo-class coverage is the Phase 3 working
//! set: the standard pseudo-elements (`::before`, `::after`,
//! `::first-line`, `::first-letter`, `::selection`, `::marker`,
//! `::placeholder`, `::backdrop`) and the standard non-tree-structural
//! pseudo-classes (user action, form UI, `:lang()`, `:dir()`). Names
//! the parser does not know are a typed
//! [`SelectorErrorKind::UnsupportedPseudoClassOrElement`] — a rule with
//! an unknown pseudo is *dropped*, which is Selectors 4 error handling
//! and exactly what keeps unknown-pseudo stylesheets from silently
//! matching nothing.

use std::borrow::Borrow;
use std::fmt as std_fmt;

use cssparser::ToCss;
use precomputed_hash::PrecomputedHash;
use selectors::parser::{
    NonTSPseudoClass as NonTSPseudoClassTrait, PseudoElement as PseudoElementTrait, SelectorImpl,
};

/// Hashes a string once, deterministically (`DefaultHasher::new` is
/// fixed-key), for the [`PrecomputedHash`] fast paths.
fn string_precomputed_hash(s: &str) -> u32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish() as u32
}

macro_rules! veyra_string_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd)]
        pub struct $name(Box<str>);

        impl $name {
            /// The wrapped string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'a> From<&'a str> for $name {
            fn from(value: &'a str) -> Self {
                $name(Box::from(value))
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl PrecomputedHash for $name {
            fn precomputed_hash(&self) -> u32 {
                string_precomputed_hash(&self.0)
            }
        }

        impl ToCss for $name {
            fn to_css<W>(&self, dest: &mut W) -> std_fmt::Result
            where
                W: std_fmt::Write,
            {
                dest.write_str(&self.0)
            }
        }
    };
}

veyra_string_type! {
    /// A selector identifier (type names, class names, ids, keywords).
    Identifier
}
veyra_string_type! {
    /// An element local name (the part after any namespace prefix).
    LocalName
}
veyra_string_type! {
    /// An attribute value in an attribute selector.
    AttrValue
}
veyra_string_type! {
    /// A namespace prefix (`svg|rect` — the part before the bar).
    /// `Default` (the empty prefix) is the no-namespace form the
    /// upstream parser uses for unprefixed names.
    NamespacePrefix
}
veyra_string_type! {
    /// A namespace URL. Batch 1 stores the parsed string; resolution
    /// against document-declared `@namespace` rules arrives with the
    /// cascade batch (the `STYLE -> URL` graph edge). `Default` is the
    /// empty namespace (no namespace), the upstream parser's unprefixed
    /// form.
    NamespaceUrl
}

/// The Veyra selector implementation binding: the associated types the
/// `selectors` crate uses for names, namespaces, and pseudos.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VeyraSelectorImpl;

impl SelectorImpl for VeyraSelectorImpl {
    /// Extra data threaded to matching context. Matching is a later
    /// batch; the unit default is the upstream requirement for parsing.
    type ExtraMatchingData<'a> = ();
    type AttrValue = AttrValue;
    type Identifier = Identifier;
    type LocalName = LocalName;
    type NamespaceUrl = NamespaceUrl;
    type NamespacePrefix = NamespacePrefix;
    type BorrowedNamespaceUrl = str;
    type BorrowedLocalName = str;
    type NonTSPseudoClass = PseudoClass;
    type PseudoElement = PseudoElement;
}

/// A standard pseudo-element (`::before`, `::after`, …).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PseudoElement {
    /// `::before`
    Before,
    /// `::after`
    After,
    /// `::first-line`
    FirstLine,
    /// `::first-letter`
    FirstLetter,
    /// `::selection`
    Selection,
    /// `::marker`
    Marker,
    /// `::placeholder`
    Placeholder,
    /// `::backdrop`
    Backdrop,
}

impl PseudoElementTrait for PseudoElement {
    /// User-action pseudo-classes apply to `::selection` and
    /// `::placeholder` (the spec's dynamic-highlight pseudo-elements).
    fn accepts_state_pseudo_classes(&self) -> bool {
        matches!(self, PseudoElement::Selection | PseudoElement::Placeholder)
    }
}

impl ToCss for PseudoElement {
    fn to_css<W>(&self, dest: &mut W) -> std_fmt::Result
    where
        W: std_fmt::Write,
    {
        let name = match self {
            PseudoElement::Before => "before",
            PseudoElement::After => "after",
            PseudoElement::FirstLine => "first-line",
            PseudoElement::FirstLetter => "first-letter",
            PseudoElement::Selection => "selection",
            PseudoElement::Marker => "marker",
            PseudoElement::Placeholder => "placeholder",
            PseudoElement::Backdrop => "backdrop",
        };
        dest.write_str("::")?;
        dest.write_str(name)
    }
}

/// A standard non-tree-structural pseudo-class.
///
/// Tree-structural pseudo-classes (`:nth-child()`, `:is()`, `:not()`,
/// …) are modeled by the `selectors` crate itself; this enum is only
/// the leaf names and the argument-carrying forms it defers to the
/// embedding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PseudoClass {
    /// `:link`
    Link,
    /// `:visited`
    Visited,
    /// `:active`
    Active,
    /// `:hover`
    Hover,
    /// `:focus`
    Focus,
    /// `:focus-visible`
    FocusVisible,
    /// `:focus-within`
    FocusWithin,
    /// `:enabled`
    Enabled,
    /// `:disabled`
    Disabled,
    /// `:checked`
    Checked,
    /// `:indeterminate`
    Indeterminate,
    /// `:placeholder-shown`
    PlaceholderShown,
    /// `:autofill`
    Autofill,
    /// `:read-only`
    ReadOnly,
    /// `:read-write`
    ReadWrite,
    /// `:required`
    Required,
    /// `:optional`
    Optional,
    /// `:valid`
    Valid,
    /// `:invalid`
    Invalid,
    /// `:user-valid`
    UserValid,
    /// `:user-invalid`
    UserInvalid,
    /// `:target`
    Target,
    /// `:lang(<language-range>#)` — the parsed language ranges
    /// (identifiers or strings, per Selectors 4); range matching is
    /// applied at style time.
    Lang(Vec<Box<str>>),
    /// `:dir(<directionality>)` — the parsed directionality keyword;
    /// matching is applied at style time.
    Dir(Box<str>),
}

impl PseudoClass {
    /// Whether this pseudo-class is `:active` or `:hover` (the
    /// upstream matcher's hover/active chain fast path).
    pub fn is_active_or_hover(&self) -> bool {
        matches!(self, PseudoClass::Active | PseudoClass::Hover)
    }

    /// Whether this pseudo-class belongs to the user-action state
    /// family (Selectors 4 §useraction-pseudos).
    pub fn is_user_action_state(&self) -> bool {
        matches!(
            self,
            PseudoClass::Active | PseudoClass::Hover | PseudoClass::Focus
        )
    }
}

impl NonTSPseudoClassTrait for PseudoClass {
    fn is_active_or_hover(&self) -> bool {
        PseudoClass::is_active_or_hover(self)
    }

    fn is_user_action_state(&self) -> bool {
        PseudoClass::is_user_action_state(self)
    }
}

impl ToCss for PseudoClass {
    fn to_css<W>(&self, dest: &mut W) -> std_fmt::Result
    where
        W: std_fmt::Write,
    {
        dest.write_str(":")?;
        match self {
            PseudoClass::Link => dest.write_str("link"),
            PseudoClass::Visited => dest.write_str("visited"),
            PseudoClass::Active => dest.write_str("active"),
            PseudoClass::Hover => dest.write_str("hover"),
            PseudoClass::Focus => dest.write_str("focus"),
            PseudoClass::FocusVisible => dest.write_str("focus-visible"),
            PseudoClass::FocusWithin => dest.write_str("focus-within"),
            PseudoClass::Enabled => dest.write_str("enabled"),
            PseudoClass::Disabled => dest.write_str("disabled"),
            PseudoClass::Checked => dest.write_str("checked"),
            PseudoClass::Indeterminate => dest.write_str("indeterminate"),
            PseudoClass::PlaceholderShown => dest.write_str("placeholder-shown"),
            PseudoClass::Autofill => dest.write_str("autofill"),
            PseudoClass::ReadOnly => dest.write_str("read-only"),
            PseudoClass::ReadWrite => dest.write_str("read-write"),
            PseudoClass::Required => dest.write_str("required"),
            PseudoClass::Optional => dest.write_str("optional"),
            PseudoClass::Valid => dest.write_str("valid"),
            PseudoClass::Invalid => dest.write_str("invalid"),
            PseudoClass::UserValid => dest.write_str("user-valid"),
            PseudoClass::UserInvalid => dest.write_str("user-invalid"),
            PseudoClass::Target => dest.write_str("target"),
            PseudoClass::Lang(ranges) => {
                dest.write_str("lang(")?;
                for (index, range) in ranges.iter().enumerate() {
                    if index > 0 {
                        dest.write_str(", ")?;
                    }
                    let mut quoted = String::new();
                    let _ = cssparser::serialize_identifier(range, &mut quoted);
                    dest.write_str(&quoted)?;
                }
                dest.write_str(")")
            }
            PseudoClass::Dir(direction) => {
                dest.write_str("dir(")?;
                dest.write_str(direction)?;
                dest.write_str(")")
            }
        }
    }
}
