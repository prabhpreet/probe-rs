//! Sequences for the nRF devices.

use super::ArmDebugSequence;
use crate::architecture::arm::ap::MemoryAp;
use crate::architecture::arm::{
    communication_interface::Initialized, ApAddress, ArmCommunicationInterface, ArmProbeInterface,
    DapAccess,
};
use crate::Memory;

pub trait Nrf: Sync + Send {
    /// Returns the ahb_ap and ctrl_ap of every core
    fn core_aps(&self, memory: &mut Memory) -> Vec<(ApAddress, ApAddress)>;

    /// Returns true when the core is unlocked and false when it is locked.
    fn is_core_unlocked(
        &self,
        arm_interface: &mut ArmCommunicationInterface<Initialized>,
        ahb_ap_address: ApAddress,
        ctrl_ap_address: ApAddress,
    ) -> Result<bool, crate::Error>;

    /// Returns true if a network core is present
    fn has_network_core(&self) -> bool;
}

const ERASEALL: u8 = 0x04;
const ERASEALLSTATUS: u8 = 0x08;

const APPLICATION_RESET_S_NETWORK_FORCEOFF_REGISTER: u32 = 0x50005614;
const RELEASE_FORCEOFF: u32 = 0;

/// Unlocks the core by performing an erase all procedure.
/// The `ap_address` must be of the ctrl ap of the core.
fn unlock_core(
    arm_interface: &mut ArmCommunicationInterface<Initialized>,
    ap_address: ApAddress,
    permissions: &crate::Permissions,
) -> Result<(), crate::Error> {
    permissions.erase_all()?;

    arm_interface.write_raw_ap_register(ap_address, ERASEALL, 1)?;
    while arm_interface.read_raw_ap_register(ap_address, ERASEALLSTATUS)? != 0 {}
    Ok(())
}

/// Sets the network core to active running.
fn set_network_core_running(interface: &mut crate::Memory) -> Result<(), crate::Error> {
    interface.write_32(
        APPLICATION_RESET_S_NETWORK_FORCEOFF_REGISTER as u64,
        &[RELEASE_FORCEOFF],
    )?;
    Ok(())
}

impl<T: Nrf> ArmDebugSequence for T {
    fn debug_device_unlock(
        &self,
        interface: &mut Box<dyn ArmProbeInterface>,
        default_ap: MemoryAp,
        permissions: &crate::Permissions,
    ) -> Result<(), crate::Error> {
        let mut interface = interface.memory_interface(default_ap)?;

        // TODO: Eraseprotect is not considered. If enabled, the debugger must set up the same keys as the firmware does
        // TODO: Approtect and Secure Approtect are not considered. If enabled, the debugger must set up the same keys as the firmware does
        // These keys should be queried from the user if required and once that mechanism is implemented

        for (core_index, (core_ahb_ap_address, core_ctrl_ap_address)) in
            self.core_aps(&mut interface).iter().copied().enumerate()
        {
            log::info!("Checking if core {} is unlocked", core_index);
            if self.is_core_unlocked(
                interface.get_arm_interface()?,
                core_ahb_ap_address,
                core_ctrl_ap_address,
            )? {
                log::info!("Core {} is already unlocked", core_index);
                continue;
            }

            log::warn!(
                "Core {} is locked. Erase procedure will be started to unlock it.",
                core_index
            );
            unlock_core(
                interface.get_arm_interface()?,
                core_ctrl_ap_address,
                permissions,
            )?;

            if !self.is_core_unlocked(
                interface.get_arm_interface()?,
                core_ahb_ap_address,
                core_ctrl_ap_address,
            )? {
                return Err(crate::Error::ArchitectureSpecific(
                    format!("Could not unlock core {}", core_index).into(),
                ));
            }
        }

        if self.has_network_core() {
            set_network_core_running(&mut interface)?;
        }

        Ok(())
    }
}
