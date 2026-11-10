use mc_bindings::Object;

/// Kept as the original object: Freecam queues these and replays them later.
#[derive(Debug, Clone)]
pub struct ClientboundPlayerInfoUpdatePacket {
    pub packet: Object,
}

// Never mutated, so the dispatch must never see it as changed.
impl PartialEq for ClientboundPlayerInfoUpdatePacket {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}
