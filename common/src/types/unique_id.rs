/// Trait that enforces the generation of a unique id across all event types
pub trait GetUniqueId {
    type UniqueId;

    /// Returns a unique id, unique to *all* event types
    fn unique_id(&self) -> Self::UniqueId;
}
