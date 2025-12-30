//! Compile-time node type specialization for search.
//!
//! Uses Rust generics to compile different versions of search functions
//! for different node types, eliminating runtime if-checks.
//!
//! # Node Types
//! - `Root`: Root of the search tree (PV=true, ROOT=true)
//! - `OnPV`: On principal variation, non-root (PV=true, ROOT=false)
//! - `OffPV`: Off principal variation, null-window (PV=false, ROOT=false)

/// Trait for compile-time node type specialization.
///
/// Implementing types provide const booleans that the compiler uses
/// to generate specialized versions of the search function.
pub trait NodeType {
    /// Whether this node is on the principal variation.
    const PV: bool;
    /// Whether this node is the root of the search tree.
    const ROOT: bool;
    /// The node type for child PV searches from this node.
    type Next: NodeType;
}

/// Root node of the search tree.
///
/// PV = true, ROOT = true
/// Next = OnPV (children are on PV but not root)
pub struct Root;

/// A node on the principal variation (non-root).
///
/// PV = true, ROOT = false
/// Next = OnPV (children remain on PV)
pub struct OnPV;

/// A node with a null window (off principal variation).
///
/// PV = false, ROOT = false
/// Next = OffPV (children remain off PV)
pub struct OffPV;

impl NodeType for Root {
    const PV: bool = true;
    const ROOT: bool = true;
    type Next = OnPV;
}

impl NodeType for OnPV {
    const PV: bool = true;
    const ROOT: bool = false;
    type Next = Self;
}

impl NodeType for OffPV {
    const PV: bool = false;
    const ROOT: bool = false;
    type Next = Self;
}
