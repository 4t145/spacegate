use serde::{Serialize, Deserialize};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpPathModifier {
    /// Type defines the type of path modifier.
    pub kind: SgHttpPathModifierType,
    /// Value is the value to be used to replace the path during forwarding.
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[serde(rename_all = "PascalCase")]
pub enum SgHttpPathModifierType {
    /// This type of modifier indicates that the full path will be replaced by the specified value.
    ReplaceFullPath,
    /// This type of modifier indicates that any prefix path matches will be replaced by the substitution value.
    /// For example, a path with a prefix match of “/foo” and a ReplacePrefixMatch substitution of “/bar” will have the “/foo” prefix replaced with “/bar” in matching requests.
    #[default]
    ReplacePrefixMatch,
}