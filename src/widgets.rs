mod device_dialog;
mod footer;
mod settings_dialog;
mod transcriptions;

pub use device_dialog::{DeviceDialog, DeviceDialogState};
pub use footer::FooterWidget;
pub use settings_dialog::{SettingsDialog, SettingsDialogState};
pub use transcriptions::{TranscriptionMessage, TranscriptionWidget, TranscriptionWidgetState};
