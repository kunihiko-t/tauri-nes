use serde::{Deserialize, Serialize};

// Define button mapping (consistent with many emulators)
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum Button {
    A,
    B,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
}

// Data structure for input events from the frontend
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputData {
    pub button: Button,
    pub pressed: bool, // true for pressed, false for released
}

// Define the controller state
#[derive(Default, Clone, Serialize)]
pub struct Controller {
    strobe: bool,       // Controller strobe signal (latches button state)
    button_index: u8,   // Index for reading button states serially
    button_states: u8,  // Current state of all 8 buttons (bitfield)
}

impl Controller {
    pub fn new() -> Self {
        Controller::default() // Use default implementation
    }

    // Write to controller register ($4016 or $4017)
    pub fn write(&mut self, data: u8) {
        self.strobe = (data & 1) == 1;
        if self.strobe {
            // Strobe high: Reset button index
            self.button_index = 0;
        }
        // The actual button states are usually set via a separate mechanism (e.g., handle_input)
    }

    // Read from controller register ($4016 or $4017)
    pub fn read(&mut self) -> u8 {
        // Only return LSB (bit 0) for button state
        // Open bus behavior for bits 1-7 is common, return 0 for simplicity
        if self.button_index > 7 {
            return 1; // All buttons read, NES returns 1
        }

        let response = (self.button_states >> self.button_index) & 1;

        // Increment index only if strobe is low
        if !self.strobe {
            self.button_index += 1;
        }

        response
    }

    // Set the state of a specific button (called by frontend input handler)
    pub fn set_button_state(&mut self, button: Button, pressed: bool) {
        let bit = match button {
            Button::A      => 0,
            Button::B      => 1,
            Button::Select => 2,
            Button::Start  => 3,
            Button::Up     => 4,
            Button::Down   => 5,
            Button::Left   => 6,
            Button::Right  => 7,
        };

        if pressed {
            self.button_states |= 1 << bit;
        } else {
            self.button_states &= !(1 << bit);
        }
    }
}