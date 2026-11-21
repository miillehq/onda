//! Owns the native target adapter and delivery permission for insertion and clipboard handoff.
use super::*;

#[derive(Default)]
pub(super) struct TargetDelivery {
    pub(super) target_service: WindowsTargetService,
}
