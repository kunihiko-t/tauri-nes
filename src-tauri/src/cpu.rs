use serde::Serialize;
use crate::bus::Bus; // Use Bus instead of Memory
// use crate::debugger::Debugger; // Debugger integration can be added later

// Status Register Flags
const CARRY_FLAG: u8 = 0b00000001;
const ZERO_FLAG: u8 = 0b00000010;
const INTERRUPT_DISABLE_FLAG: u8 = 0b00000100;
const DECIMAL_MODE_FLAG: u8 = 0b00001000; // Not used in NES
const BREAK_FLAG: u8 = 0b00010000;
const UNUSED_FLAG: u8 = 0b00100000; // Always set
const OVERFLOW_FLAG: u8 = 0b01000000;
const NEGATIVE_FLAG: u8 = 0b10000000;

// Structure to hold CPU state for inspection (e.g., via Tauri)
#[derive(Serialize, Clone, Debug)]
pub struct InspectState {
    pub accumulator: u8,
    pub x_register: u8,
    pub y_register: u8,
    pub status: u8,
    pub program_counter: u16,
    pub stack_pointer: u8,
}

// The 6502 CPU core
#[derive(Debug)] // Added Debug derive for easier inspection if needed
pub struct Cpu6502 {
    accumulator: u8,
    x_register: u8,
    y_register: u8,
    status: u8,
    program_counter: u16,
    stack_pointer: u8,
    // cycles: u64, // Cycle counter for timing (implement later)
}

// Addressing modes enum
#[derive(Debug, Copy, Clone)] // Added derives
enum AddressingMode {
    Implied, // Added Implied for instructions like INX, DEX, CLC etc.
    Accumulator,
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Relative,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Indirect,
    IndexedIndirect, // (Indirect, X)
    IndirectIndexed, // (Indirect), Y
}

impl Cpu6502 {
    pub fn new() -> Self {
        Self {
            accumulator: 0,
            x_register: 0,
            y_register: 0,
            status: UNUSED_FLAG | INTERRUPT_DISABLE_FLAG, // Start with unused and interrupt disable flags set
            program_counter: 0, // Will be set by reset vector
            stack_pointer: 0xFD, // Standard 6502 stack pointer init
            // cycles: 0,
        }
    }

    // Reset the CPU to its initial state
    pub fn reset(&mut self, bus: &mut Bus) { // Changed memory to bus
        self.accumulator = 0;
        self.x_register = 0;
        self.y_register = 0;
        self.stack_pointer = 0xFD;
        self.status = UNUSED_FLAG | INTERRUPT_DISABLE_FLAG;

        // Read the reset vector from the bus
        self.program_counter = self.read_u16(bus, 0xFFFC); // Changed memory to bus
        // self.cycles = 7; // Reset takes 7 cycles
    }

    // --- Bus Access --- (Renamed from Memory Access)
    // Reads a byte from the bus
    fn read(&self, bus: &mut Bus, addr: u16) -> u8 { // Takes &mut Bus
        bus.read(addr)
    }

    // Writes a byte to the bus
    fn write(&mut self, bus: &mut Bus, addr: u16, data: u8) {
        // --- DEBUG PPU Write ---
        if addr >= 0x2000 && addr <= 0x3FFF { // Check if address is in PPU register/mirror range
            println!(
                "[CPU Write] Addr: {:04X}, Data: {:02X} (PC: {:04X}, Cycle: {})", // Assuming Bus has total_cycles
                addr, data, self.program_counter, bus.total_cycles // You might need to add total_cycles to Bus or pass it differently
            );
        }
        // --- END DEBUG ---
        bus.write(addr, data);
        // self.cycles += 1; // Removed: Cycle counting is handled by step method
                          // Actual cycles depend on where the write occurs, but adding 1 is common placeholder
    }

    // Reads a 16-bit word (little-endian) from the bus
    fn read_u16(&self, bus: &mut Bus, addr: u16) -> u16 { // Takes &mut Bus
        let lo = self.read(bus, addr) as u16;
        let hi = self.read(bus, addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    // --- Stack Operations ---
    fn push(&mut self, bus: &mut Bus, value: u8) { // Changed memory to bus
        self.write(bus, 0x0100 + self.stack_pointer as u16, value);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1);
    }

    fn pull(&mut self, bus: &mut Bus) -> u8 { // Already takes &mut Bus
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.read(bus, 0x0100 + self.stack_pointer as u16)
    }

    // --- Flag Updates ---
    fn update_nz_flags(&mut self, value: u8) {
        if value == 0 {
            self.status |= ZERO_FLAG;
        } else {
            self.status &= !ZERO_FLAG;
        }
        if value & NEGATIVE_FLAG != 0 {
            self.status |= NEGATIVE_FLAG;
        } else {
            self.status &= !NEGATIVE_FLAG;
        }
    }

    // --- Addressing Mode Implementations ---
    // Returns the effective address for a given mode, and potentially reads the operand value
    // Also increments PC as needed. Returns the calculated address.
    // Note: Relative mode calculates the *target* address.
    // Implied/Accumulator modes should not call this.
    fn get_operand_address(&mut self, bus: &mut Bus, mode: AddressingMode) -> u16 { // Takes &mut Bus
        match mode {
            AddressingMode::Immediate => {
                let addr = self.program_counter;
                self.program_counter = self.program_counter.wrapping_add(1);
                addr // Address of the immediate value itself
            }
            AddressingMode::ZeroPage => {
                let addr = self.read(bus, self.program_counter) as u16;
                self.program_counter = self.program_counter.wrapping_add(1);
                addr
            }
            AddressingMode::ZeroPageX => {
                let base = self.read(bus, self.program_counter);
                self.program_counter = self.program_counter.wrapping_add(1);
                base.wrapping_add(self.x_register) as u16
            }
            AddressingMode::ZeroPageY => {
                let base = self.read(bus, self.program_counter);
                self.program_counter = self.program_counter.wrapping_add(1);
                base.wrapping_add(self.y_register) as u16
            }
            AddressingMode::Absolute => {
                let addr = self.read_u16(bus, self.program_counter);
                self.program_counter = self.program_counter.wrapping_add(2);
                addr
            }
            AddressingMode::AbsoluteX => {
                let base = self.read_u16(bus, self.program_counter);
                self.program_counter = self.program_counter.wrapping_add(2);
                // TODO: Add cycle penalty if page crossed
                base.wrapping_add(self.x_register as u16)
            }
            AddressingMode::AbsoluteY => {
                let base = self.read_u16(bus, self.program_counter);
                self.program_counter = self.program_counter.wrapping_add(2);
                // TODO: Add cycle penalty if page crossed
                base.wrapping_add(self.y_register as u16)
            }
            AddressingMode::Indirect => { // Only used by JMP
                let ptr_addr = self.read_u16(bus, self.program_counter);
                self.program_counter = self.program_counter.wrapping_add(2);
                // Replicate 6502 bug: if low byte is FF, high byte wraps without incrementing page
                let effective_addr = if ptr_addr & 0x00FF == 0x00FF {
                    let lo = self.read(bus, ptr_addr) as u16;
                    let hi = self.read(bus, ptr_addr & 0xFF00) as u16; // Read from same page
                    (hi << 8) | lo
                } else {
                    self.read_u16(bus, ptr_addr)
                };
                effective_addr
            }
            AddressingMode::IndexedIndirect => { // (Indirect, X)
                let base = self.read(bus, self.program_counter);
                self.program_counter = self.program_counter.wrapping_add(1);
                let ptr = base.wrapping_add(self.x_register);
                let lo = self.read(bus, ptr as u16) as u16;
                let hi = self.read(bus, ptr.wrapping_add(1) as u16) as u16;
                (hi << 8) | lo
            }
            AddressingMode::IndirectIndexed => { // (Indirect), Y
                let base = self.read(bus, self.program_counter);
                self.program_counter = self.program_counter.wrapping_add(1);
                let lo = self.read(bus, base as u16) as u16;
                let hi = self.read(bus, base.wrapping_add(1) as u16) as u16;
                let ptr_addr = (hi << 8) | lo;
                // TODO: Add cycle penalty if page crossed
                ptr_addr.wrapping_add(self.y_register as u16)
            }
            AddressingMode::Relative => {
                let offset = self.read(bus, self.program_counter) as i8; // Read offset as signed i8
                self.program_counter = self.program_counter.wrapping_add(1);
                // Calculate target address relative to the *next* instruction's address
                self.program_counter.wrapping_add(offset as u16) // wrapping_add handles signed addition correctly
            }
            // These modes don't produce an address in the same way
            AddressingMode::Implied | AddressingMode::Accumulator => {
                panic!("Implied/Accumulator mode should not call get_operand_address");
                // Or return a dummy value like 0, but the instruction logic must handle it.
            }
        }
    }

    // Helper to fetch operand based on addressing mode
    fn fetch_operand(&mut self, bus: &mut Bus, mode: AddressingMode) -> u8 { // Takes &mut Bus
        match mode {
            AddressingMode::Accumulator => self.accumulator,
            AddressingMode::Immediate => {
                let addr = self.get_operand_address(bus, mode); // Get address (advances PC)
                self.read(bus, addr) // Now read using the address
            }
            _ => {
                let addr = self.get_operand_address(bus, mode); // Pass bus
                self.read(bus, addr) // Pass bus
            }
        }
    }


    // --- Instruction Implementations ---

    // ADC - Add with Carry
    fn adc(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        let carry = (self.status & CARRY_FLAG) as u16;
        let result = self.accumulator as u16 + operand as u16 + carry;

        // Set Carry flag
        if result > 0xFF { self.status |= CARRY_FLAG; } else { self.status &= !CARRY_FLAG; }

        // Set Overflow flag
        if (self.accumulator ^ (result as u8)) & (operand ^ (result as u8)) & NEGATIVE_FLAG != 0 {
            self.status |= OVERFLOW_FLAG;
        } else {
            self.status &= !OVERFLOW_FLAG;
        }

        self.accumulator = result as u8;
        self.update_nz_flags(self.accumulator);
    }

    // AND - Logical AND
    fn and(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.accumulator &= operand;
        self.update_nz_flags(self.accumulator);
    }

    // ASL - Arithmetic Shift Left
    fn asl(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let value = match mode {
            AddressingMode::Accumulator => {
                let acc = self.accumulator;
                self.status = (self.status & !CARRY_FLAG) | ((acc & NEGATIVE_FLAG) >> 7); // Old bit 7 to Carry
                self.accumulator = acc << 1;
                self.accumulator
            }
            _ => {
                let addr = self.get_operand_address(bus, mode); // Pass bus
                let operand = self.read(bus, addr); // Pass bus
                self.status = (self.status & !CARRY_FLAG) | ((operand & NEGATIVE_FLAG) >> 7); // Old bit 7 to Carry
                let result = operand << 1;
                self.write(bus, addr, result); // Pass bus
                result
            }
        };
        self.update_nz_flags(value);
    }

    // Branching helper
    fn branch(&mut self, bus: &mut Bus, condition: bool, mode: AddressingMode) { // Takes &mut Bus
         if condition {
            let target_addr = self.get_operand_address(bus, mode); // Pass bus
            // TODO: Add cycle penalty if page boundary is crossed
            self.program_counter = target_addr;
        } else {
             // Consume the relative offset byte even if branch not taken
             let _offset = self.read(bus, self.program_counter); // Pass bus
             self.program_counter = self.program_counter.wrapping_add(1);
        }
    }

    // BCC - Branch if Carry Clear
    fn bcc(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.status & CARRY_FLAG == 0, mode); // Pass bus
    }
    // BCS - Branch if Carry Set
    fn bcs(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.status & CARRY_FLAG != 0, mode); // Pass bus
    }
    // BEQ - Branch if Equal (Zero flag set)
    fn beq(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.status & ZERO_FLAG != 0, mode); // Pass bus
    }
    // BNE - Branch if Not Equal (Zero flag clear)
    fn bne(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.status & ZERO_FLAG == 0, mode); // Pass bus
    }
    // BMI - Branch if Minus (Negative flag set)
    fn bmi(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.status & NEGATIVE_FLAG != 0, mode); // Pass bus
    }
    // BPL - Branch if Positive (Negative flag clear)
    fn bpl(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.status & NEGATIVE_FLAG == 0, mode); // Pass bus
    }
    // BVC - Branch if Overflow Clear
    fn bvc(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.status & OVERFLOW_FLAG == 0, mode); // Pass bus
    }
    // BVS - Branch if Overflow Set
    fn bvs(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.status & OVERFLOW_FLAG != 0, mode); // Pass bus
    }

    // BIT - Test Bits
    fn bit(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        // Zero flag: set if result of A & M is zero
        if self.accumulator & operand == 0 { self.status |= ZERO_FLAG; } else { self.status &= !ZERO_FLAG; }
        // Negative flag: set to bit 7 of M
        if operand & NEGATIVE_FLAG != 0 { self.status |= NEGATIVE_FLAG; } else { self.status &= !NEGATIVE_FLAG; }
        // Overflow flag: set to bit 6 of M
        if operand & OVERFLOW_FLAG != 0 { self.status |= OVERFLOW_FLAG; } else { self.status &= !OVERFLOW_FLAG; }
    }

    // BRK - Force Interrupt
    fn brk(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus
        self.program_counter = self.program_counter.wrapping_add(1); // BRK has a padding byte
        self.push(bus, (self.program_counter >> 8) as u8); // Pass bus
        self.push(bus, self.program_counter as u8); // Pass bus
        let status_with_break = self.status | BREAK_FLAG | UNUSED_FLAG;
        self.push(bus, status_with_break); // Pass bus
        self.status |= INTERRUPT_DISABLE_FLAG; // Set interrupt disable flag
        self.program_counter = self.read_u16(bus, 0xFFFE); // Load IRQ vector, Pass bus
    }

    // CLC - Clear Carry Flag
    fn clc(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.status &= !CARRY_FLAG;
    }
    // CLD - Clear Decimal Mode Flag (No-op on NES)
    fn cld(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.status &= !DECIMAL_MODE_FLAG;
    }
    // CLI - Clear Interrupt Disable Flag
    fn cli(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.status &= !INTERRUPT_DISABLE_FLAG;
    }
    // CLV - Clear Overflow Flag
    fn clv(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.status &= !OVERFLOW_FLAG;
    }

    // Compare helper (doesn't need bus access)
    fn compare(&mut self, register_value: u8, memory_value: u8) {
        let result = register_value.wrapping_sub(memory_value);
        if register_value >= memory_value { self.status |= CARRY_FLAG; } else { self.status &= !CARRY_FLAG; }
        self.update_nz_flags(result);
    }
    // CMP - Compare Accumulator
    fn cmp(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.compare(self.accumulator, operand);
    }
    // CPX - Compare X Register
    fn cpx(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.compare(self.x_register, operand);
    }
    // CPY - Compare Y Register
    fn cpy(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.compare(self.y_register, operand);
    }

    // DEC - Decrement Memory
    fn dec(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        let value = self.read(bus, addr).wrapping_sub(1); // Pass bus
        self.write(bus, addr, value); // Pass bus
        self.update_nz_flags(value);
    }
    // DEX - Decrement X Register
    fn dex(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.x_register = self.x_register.wrapping_sub(1);
        self.update_nz_flags(self.x_register);
    }
    // DEY - Decrement Y Register
    fn dey(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.y_register = self.y_register.wrapping_sub(1);
        self.update_nz_flags(self.y_register);
    }

    // EOR - Exclusive OR
    fn eor(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.accumulator ^= operand;
        self.update_nz_flags(self.accumulator);
    }

    // INC - Increment Memory
    fn inc(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        let value = self.read(bus, addr).wrapping_add(1); // Pass bus
        self.write(bus, addr, value); // Pass bus
        self.update_nz_flags(value);
    }
    // INX - Increment X Register
    fn inx(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.x_register = self.x_register.wrapping_add(1);
        self.update_nz_flags(self.x_register);
    }
    // INY - Increment Y Register
    fn iny(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.y_register = self.y_register.wrapping_add(1);
        self.update_nz_flags(self.y_register);
    }

    // JMP - Jump
    fn jmp(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.program_counter = self.get_operand_address(bus, mode); // Pass bus
    }
    // JSR - Jump to Subroutine
    fn jsr(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let target_addr = self.get_operand_address(bus, mode); // Pass bus
        let return_addr = self.program_counter - 1; // JSR pushes PC-1
        self.push(bus, (return_addr >> 8) as u8); // Pass bus
        self.push(bus, return_addr as u8);       // Pass bus
        self.program_counter = target_addr;
    }

    // LDA - Load Accumulator
    fn lda(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let value = self.fetch_operand(bus, mode); // Pass bus
        self.accumulator = value;
        self.update_nz_flags(self.accumulator);
    }
    // LDX - Load X Register
    fn ldx(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let value = self.fetch_operand(bus, mode); // Pass bus
        self.x_register = value;
        self.update_nz_flags(self.x_register);
    }
    // LDY - Load Y Register
    fn ldy(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let value = self.fetch_operand(bus, mode); // Pass bus
        self.y_register = value;
        self.update_nz_flags(self.y_register);
    }

    // LSR - Logical Shift Right
    fn lsr(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
         let value = match mode {
            AddressingMode::Accumulator => {
                let acc = self.accumulator;
                self.status = (self.status & !CARRY_FLAG) | (acc & CARRY_FLAG); // Old bit 0 to Carry
                self.accumulator = acc >> 1;
                self.accumulator
            }
            _ => {
                let addr = self.get_operand_address(bus, mode); // Pass bus
                let operand = self.read(bus, addr); // Pass bus
                self.status = (self.status & !CARRY_FLAG) | (operand & CARRY_FLAG); // Old bit 0 to Carry
                let result = operand >> 1;
                self.write(bus, addr, result); // Pass bus
                result
            }
        };
        self.update_nz_flags(value); // Negative is always 0 after LSR
        self.status &= !NEGATIVE_FLAG; // Explicitly clear negative flag
    }

    // NOP - No Operation
    fn nop(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        // Do nothing
        // Some unofficial NOPs might consume operands, handle later if needed
    }

    // ORA - Logical Inclusive OR
    fn ora(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.accumulator |= operand;
        self.update_nz_flags(self.accumulator);
    }

    // PHA - Push Accumulator
    fn pha(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus
        self.push(bus, self.accumulator); // Pass bus
    }
    // PHP - Push Processor Status
    fn php(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus
        // Note: Pushed status has Break and Unused flags set
        let status_with_break = self.status | BREAK_FLAG | UNUSED_FLAG;
        self.push(bus, status_with_break); // Pass bus
    }
    // PLA - Pull Accumulator
    fn pla(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus
        self.accumulator = self.pull(bus); // Pass bus
        self.update_nz_flags(self.accumulator);
    }
    // PLP - Pull Processor Status
    fn plp(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus
        self.status = self.pull(bus); // Pass bus
        self.status &= !BREAK_FLAG; // Break flag is ignored when pulled
        self.status |= UNUSED_FLAG; // Unused flag is always set
    }

    // ROL - Rotate Left
    fn rol(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let carry_in = self.status & CARRY_FLAG;
        let value = match mode {
            AddressingMode::Accumulator => {
                let acc = self.accumulator;
                self.status = (self.status & !CARRY_FLAG) | ((acc & NEGATIVE_FLAG) >> 7); // Old bit 7 to Carry
                self.accumulator = (acc << 1) | carry_in;
                self.accumulator
            }
            _ => {
                let addr = self.get_operand_address(bus, mode); // Pass bus
                let operand = self.read(bus, addr); // Pass bus
                self.status = (self.status & !CARRY_FLAG) | ((operand & NEGATIVE_FLAG) >> 7); // Old bit 7 to Carry
                let result = (operand << 1) | carry_in;
                self.write(bus, addr, result); // Pass bus
                result
            }
        };
        self.update_nz_flags(value);
    }

    // ROR - Rotate Right
    fn ror(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let carry_in = (self.status & CARRY_FLAG) << 7; // Carry to bit 7
         let value = match mode {
            AddressingMode::Accumulator => {
                let acc = self.accumulator;
                self.status = (self.status & !CARRY_FLAG) | (acc & CARRY_FLAG); // Old bit 0 to Carry
                self.accumulator = (acc >> 1) | carry_in;
                self.accumulator
            }
            _ => {
                let addr = self.get_operand_address(bus, mode); // Pass bus
                let operand = self.read(bus, addr); // Pass bus
                self.status = (self.status & !CARRY_FLAG) | (operand & CARRY_FLAG); // Old bit 0 to Carry
                let result = (operand >> 1) | carry_in;
                self.write(bus, addr, result); // Pass bus
                result
            }
        };
        self.update_nz_flags(value);
    }

    // RTI - Return from Interrupt
    fn rti(&mut self, bus: &mut Bus) { // Changed memory to bus
        self.plp(bus, AddressingMode::Implied); // Pass bus
        let lo = self.pull(bus) as u16; // Pass bus
        let hi = self.pull(bus) as u16; // Pass bus
        self.program_counter = (hi << 8) | lo;
    }

    // RTS - Return from Subroutine
    fn rts(&mut self, bus: &mut Bus) { // Changed memory to bus
        let lo = self.pull(bus) as u16; // Pass bus
        let hi = self.pull(bus) as u16; // Pass bus
        self.program_counter = ((hi << 8) | lo).wrapping_add(1); // RTS pulls PC-1, add 1
    }

    // SBC - Subtract with Carry
    fn sbc(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        // SBC is effectively ADC with the operand bitwise inverted
        let operand = self.fetch_operand(bus, mode); // Pass bus
        let inverted_operand = !operand;

        let carry = (self.status & CARRY_FLAG) as u16; // Carry acts as NOT borrow
        let result = self.accumulator as u16 + inverted_operand as u16 + carry;

        // Set Carry flag (Set if no borrow occurred, i.e., result >= 0x100)
        if result > 0xFF { self.status |= CARRY_FLAG; } else { self.status &= !CARRY_FLAG; }

        // Set Overflow flag
        if (self.accumulator ^ (result as u8)) & (inverted_operand ^ (result as u8)) & NEGATIVE_FLAG != 0 {
            self.status |= OVERFLOW_FLAG;
        } else {
            self.status &= !OVERFLOW_FLAG;
        }

        self.accumulator = result as u8;
        self.update_nz_flags(self.accumulator);
    }

    // SEC - Set Carry Flag
    fn sec(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.status |= CARRY_FLAG;
    }
    // SED - Set Decimal Mode Flag (No-op on NES)
    fn sed(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.status |= DECIMAL_MODE_FLAG;
    }
    // SEI - Set Interrupt Disable Flag
    fn sei(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.status |= INTERRUPT_DISABLE_FLAG;
    }

    // STA - Store Accumulator
    fn sta(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        self.write(bus, addr, self.accumulator); // Pass bus
    }
    // STX - Store X Register
    fn stx(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        self.write(bus, addr, self.x_register); // Pass bus
    }
    // STY - Store Y Register
    fn sty(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        self.write(bus, addr, self.y_register); // Pass bus
    }

    // TAX - Transfer Accumulator to X
    fn tax(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.x_register = self.accumulator;
        self.update_nz_flags(self.x_register);
    }
    // TAY - Transfer Accumulator to Y
    fn tay(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.y_register = self.accumulator;
        self.update_nz_flags(self.y_register);
    }
    // TSX - Transfer Stack Pointer to X
    fn tsx(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.x_register = self.stack_pointer;
        self.update_nz_flags(self.x_register);
    }
    // TXA - Transfer X to Accumulator
    fn txa(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.accumulator = self.x_register;
        self.update_nz_flags(self.accumulator);
    }
    // TXS - Transfer X to Stack Pointer
    fn txs(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.stack_pointer = self.x_register;
        // TXS does not update flags
    }
    // TYA - Transfer Y to Accumulator
    fn tya(&mut self, _bus: &Bus, _mode: AddressingMode) { // Changed memory to bus (ignored)
        self.accumulator = self.y_register;
        self.update_nz_flags(self.accumulator);
    }

    // --- Undocumented Opcodes ---
    // Implement later if needed (e.g., LAX, SAX, DCP, ISB, SLO, RLA, SRE, RRA)
    fn lax(&mut self, _bus: &Bus, _mode: AddressingMode) { unimplemented!("Unimplemented opcode LAX"); } // Changed memory to bus (ignored)
    fn sax(&mut self, _bus: &mut Bus, _mode: AddressingMode) { unimplemented!("Unimplemented opcode SAX"); } // Changed memory to bus (ignored)
    // ... and so on


    // --- Execution Cycle ---
    // Executes a single CPU instruction. Returns the number of cycles consumed.
    pub fn step(&mut self, bus: &mut Bus) -> u8 {
        // --- Store state *before* execution for logging ---
        let pc_before = self.program_counter;
        let _a_before = self.accumulator;
        let _x_before = self.x_register;
        let _y_before = self.y_register;
        let _p_before = self.status;
        let _sp_before = self.stack_pointer;
        let _cycles_before = bus.total_cycles; // Get cycle count before step

        // --- Interrupt Handling (TODO) ---
        // TODO: Handle interrupts (NMI, IRQ) before fetching opcode

        let opcode = self.read(bus, self.program_counter);
        let pc_after_fetch = self.program_counter.wrapping_add(1);

        let (mode, base_cycles) = match opcode {
            // ADC
            0x69 => (AddressingMode::Immediate, 2), 0x65 => (AddressingMode::ZeroPage, 3), 0x75 => (AddressingMode::ZeroPageX, 4),
            0x6D => (AddressingMode::Absolute, 4), 0x7D => (AddressingMode::AbsoluteX, 4/*+1 page cross*/), 0x79 => (AddressingMode::AbsoluteY, 4/*+1 page cross*/),
            0x61 => (AddressingMode::IndexedIndirect, 6), 0x71 => (AddressingMode::IndirectIndexed, 5/*+1 page cross*/),
            // AND
            0x29 => (AddressingMode::Immediate, 2), 0x25 => (AddressingMode::ZeroPage, 3), 0x35 => (AddressingMode::ZeroPageX, 4),
            0x2D => (AddressingMode::Absolute, 4), 0x3D => (AddressingMode::AbsoluteX, 4/*+1 page cross*/), 0x39 => (AddressingMode::AbsoluteY, 4/*+1 page cross*/),
            0x21 => (AddressingMode::IndexedIndirect, 6), 0x31 => (AddressingMode::IndirectIndexed, 5/*+1 page cross*/),
            // ASL
            0x0A => (AddressingMode::Accumulator, 2), 0x06 => (AddressingMode::ZeroPage, 5), 0x16 => (AddressingMode::ZeroPageX, 6),
            0x0E => (AddressingMode::Absolute, 6), 0x1E => (AddressingMode::AbsoluteX, 7),
            // Branch Instructions
            0x90 => (AddressingMode::Relative, 2), // BCC
            0xB0 => (AddressingMode::Relative, 2), // BCS
            0xF0 => (AddressingMode::Relative, 2), // BEQ
            0x30 => (AddressingMode::Relative, 2), // BMI
            0xD0 => (AddressingMode::Relative, 2), // BNE
            0x10 => (AddressingMode::Relative, 2), // BPL
            0x50 => (AddressingMode::Relative, 2), // BVC
            0x70 => (AddressingMode::Relative, 2), // BVS
            // BIT
            0x24 => (AddressingMode::ZeroPage, 3), 0x2C => (AddressingMode::Absolute, 4),
            // BRK
            0x00 => (AddressingMode::Implied, 7),
            // Flag Instructions (Implied)
            0x18 => (AddressingMode::Implied, 2), // CLC
            0xD8 => (AddressingMode::Implied, 2), // CLD
            0x58 => (AddressingMode::Implied, 2), // CLI
            0xB8 => (AddressingMode::Implied, 2), // CLV
            0x38 => (AddressingMode::Implied, 2), // SEC
            0xF8 => (AddressingMode::Implied, 2), // SED
            0x78 => (AddressingMode::Implied, 2), // SEI
            // CMP
            0xC9 => (AddressingMode::Immediate, 2), 0xC5 => (AddressingMode::ZeroPage, 3), 0xD5 => (AddressingMode::ZeroPageX, 4),
            0xCD => (AddressingMode::Absolute, 4), 0xDD => (AddressingMode::AbsoluteX, 4/*+1 page cross*/), 0xD9 => (AddressingMode::AbsoluteY, 4/*+1 page cross*/),
            0xC1 => (AddressingMode::IndexedIndirect, 6), 0xD1 => (AddressingMode::IndirectIndexed, 5/*+1 page cross*/),
            // CPX
            0xE0 => (AddressingMode::Immediate, 2), 0xE4 => (AddressingMode::ZeroPage, 3), 0xEC => (AddressingMode::Absolute, 4),
            // CPY
            0xC0 => (AddressingMode::Immediate, 2), 0xC4 => (AddressingMode::ZeroPage, 3), 0xCC => (AddressingMode::Absolute, 4),
            // DEC
            0xC6 => (AddressingMode::ZeroPage, 5), 0xD6 => (AddressingMode::ZeroPageX, 6), 0xCE => (AddressingMode::Absolute, 6), 0xDE => (AddressingMode::AbsoluteX, 7),
            // DEX, DEY (Implied)
            0xCA => (AddressingMode::Implied, 2), // DEX
            0x88 => (AddressingMode::Implied, 2), // DEY
            // EOR
            0x49 => (AddressingMode::Immediate, 2), 0x45 => (AddressingMode::ZeroPage, 3), 0x55 => (AddressingMode::ZeroPageX, 4),
            0x4D => (AddressingMode::Absolute, 4), 0x5D => (AddressingMode::AbsoluteX, 4/*+1 page cross*/), 0x59 => (AddressingMode::AbsoluteY, 4/*+1 page cross*/),
            0x41 => (AddressingMode::IndexedIndirect, 6), 0x51 => (AddressingMode::IndirectIndexed, 5/*+1 page cross*/),
            // INC
            0xE6 => (AddressingMode::ZeroPage, 5), 0xF6 => (AddressingMode::ZeroPageX, 6), 0xEE => (AddressingMode::Absolute, 6), 0xFE => (AddressingMode::AbsoluteX, 7),
            // INX, INY (Implied)
            0xE8 => (AddressingMode::Implied, 2), // INX
            0xC8 => (AddressingMode::Implied, 2), // INY
            // JMP
            0x4C => (AddressingMode::Absolute, 3), 0x6C => (AddressingMode::Indirect, 5),
            // JSR
            0x20 => (AddressingMode::Absolute, 6),
            // LDA
            0xA9 => (AddressingMode::Immediate, 2), 0xA5 => (AddressingMode::ZeroPage, 3), 0xB5 => (AddressingMode::ZeroPageX, 4),
            0xAD => (AddressingMode::Absolute, 4), 0xBD => (AddressingMode::AbsoluteX, 4/*+1 page cross*/), 0xB9 => (AddressingMode::AbsoluteY, 4/*+1 page cross*/),
            0xA1 => (AddressingMode::IndexedIndirect, 6), 0xB1 => (AddressingMode::IndirectIndexed, 5/*+1 page cross*/),
            // LDX
            0xA2 => (AddressingMode::Immediate, 2), 0xA6 => (AddressingMode::ZeroPage, 3), 0xB6 => (AddressingMode::ZeroPageY, 4),
            0xAE => (AddressingMode::Absolute, 4), 0xBE => (AddressingMode::AbsoluteY, 4/*+1 page cross*/),
            // LDY
            0xA0 => (AddressingMode::Immediate, 2), 0xA4 => (AddressingMode::ZeroPage, 3), 0xB4 => (AddressingMode::ZeroPageX, 4),
            0xAC => (AddressingMode::Absolute, 4), 0xBC => (AddressingMode::AbsoluteX, 4/*+1 page cross*/),
            // LSR
            0x4A => (AddressingMode::Accumulator, 2), 0x46 => (AddressingMode::ZeroPage, 5), 0x56 => (AddressingMode::ZeroPageX, 6),
            0x4E => (AddressingMode::Absolute, 6), 0x5E => (AddressingMode::AbsoluteX, 7),
            // NOP
            0xEA => (AddressingMode::Implied, 2),
            // ORA
            0x09 => (AddressingMode::Immediate, 2), 0x05 => (AddressingMode::ZeroPage, 3), 0x15 => (AddressingMode::ZeroPageX, 4),
            0x0D => (AddressingMode::Absolute, 4), 0x1D => (AddressingMode::AbsoluteX, 4/*+1 page cross*/), 0x19 => (AddressingMode::AbsoluteY, 4/*+1 page cross*/),
            0x01 => (AddressingMode::IndexedIndirect, 6), 0x11 => (AddressingMode::IndirectIndexed, 5/*+1 page cross*/),
            // Stack Instructions (Implied)
            0x48 => (AddressingMode::Implied, 3), // PHA
            0x08 => (AddressingMode::Implied, 3), // PHP
            0x68 => (AddressingMode::Implied, 4), // PLA
            0x28 => (AddressingMode::Implied, 4), // PLP
            // ROL
            0x2A => (AddressingMode::Accumulator, 2), 0x26 => (AddressingMode::ZeroPage, 5), 0x36 => (AddressingMode::ZeroPageX, 6),
            0x2E => (AddressingMode::Absolute, 6), 0x3E => (AddressingMode::AbsoluteX, 7),
            // ROR
            0x6A => (AddressingMode::Accumulator, 2), 0x66 => (AddressingMode::ZeroPage, 5), 0x76 => (AddressingMode::ZeroPageX, 6),
            0x6E => (AddressingMode::Absolute, 6), 0x7E => (AddressingMode::AbsoluteX, 7),
            // RTI, RTS (Implied)
            0x40 => (AddressingMode::Implied, 6), // RTI
            0x60 => (AddressingMode::Implied, 6), // RTS
            // SBC
            0xE9 => (AddressingMode::Immediate, 2), 0xE5 => (AddressingMode::ZeroPage, 3), 0xF5 => (AddressingMode::ZeroPageX, 4),
            0xED => (AddressingMode::Absolute, 4), 0xFD => (AddressingMode::AbsoluteX, 4/*+1 page cross*/), 0xF9 => (AddressingMode::AbsoluteY, 4/*+1 page cross*/),
            0xE1 => (AddressingMode::IndexedIndirect, 6), 0xF1 => (AddressingMode::IndirectIndexed, 5/*+1 page cross*/),
            // STA
            0x85 => (AddressingMode::ZeroPage, 3), 0x95 => (AddressingMode::ZeroPageX, 4),
            0x8D => (AddressingMode::Absolute, 4), 0x9D => (AddressingMode::AbsoluteX, 5), 0x99 => (AddressingMode::AbsoluteY, 5),
            0x81 => (AddressingMode::IndexedIndirect, 6), 0x91 => (AddressingMode::IndirectIndexed, 6),
            // STX
            0x86 => (AddressingMode::ZeroPage, 3), 0x96 => (AddressingMode::ZeroPageY, 4), 0x8E => (AddressingMode::Absolute, 4),
            // STY
            0x84 => (AddressingMode::ZeroPage, 3), 0x94 => (AddressingMode::ZeroPageX, 4), 0x8C => (AddressingMode::Absolute, 4),
            // Transfer Instructions (Implied)
            0xAA => (AddressingMode::Implied, 2), // TAX
            0xA8 => (AddressingMode::Implied, 2), // TAY
            0xBA => (AddressingMode::Implied, 2), // TSX
            0x8A => (AddressingMode::Implied, 2), // TXA
            0x9A => (AddressingMode::Implied, 2), // TXS
            0x98 => (AddressingMode::Implied, 2), // TYA

            // Unofficial Opcodes (Treat as NOP for now, or panic/implement later)
            // Example: NOPs
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => (AddressingMode::Implied, 2),
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => (AddressingMode::Immediate, 2), // DOP (NOP imm)
            0x04 | 0x44 | 0x64 => (AddressingMode::ZeroPage, 3), // DOP (NOP zp)
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => (AddressingMode::ZeroPageX, 4), // DOP (NOP zp,x)
            0x0C => (AddressingMode::Absolute, 4), // TOP (NOP abs)
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => (AddressingMode::AbsoluteX, 4/*+1 page cross*/), // TOP (NOP abs,x)
            // LAX
            0xA7 => (AddressingMode::ZeroPage, 3), 0xB7 => (AddressingMode::ZeroPageY, 4),
            0xAF => (AddressingMode::Absolute, 4), 0xBF => (AddressingMode::AbsoluteY, 4/*+1 page cross*/),
            0xA3 => (AddressingMode::IndexedIndirect, 6), 0xB3 => (AddressingMode::IndirectIndexed, 5/*+1 page cross*/),
            // SAX
            0x87 => (AddressingMode::ZeroPage, 3), 0x97 => (AddressingMode::ZeroPageY, 4),
            0x8F => (AddressingMode::Absolute, 4), 0x83 => (AddressingMode::IndexedIndirect, 6),
             // Others (Just NOP them for now to avoid panic)
            0xEB => (AddressingMode::Immediate, 2), // Unofficial SBC
            // Add more unofficial NOPs or specific implementations later

             _ => {
                 panic!("Unknown opcode: {:02X} at PC: {:04X}", opcode, pc_before);
             }
        };

        // --- Store operand bytes for logging ---
        // This is tricky, need to read bytes *without* advancing PC yet for the log
        // Let's simplify for now and just log opcode, maybe read operand bytes later if needed
        // Or, reconstruct based on addressing mode *after* execution (more complex)
        let operand1 = if matches!(mode, AddressingMode::ZeroPage | AddressingMode::ZeroPageX | AddressingMode::ZeroPageY | AddressingMode::Immediate | AddressingMode::IndexedIndirect | AddressingMode::IndirectIndexed | AddressingMode::Relative) {
            bus.read(pc_after_fetch) // Read potential first operand byte
        } else if matches!(mode, AddressingMode::Absolute | AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::Indirect) {
             bus.read(pc_after_fetch)
        } else { 0x00 }; // Implied, Accumulator

        let operand2 = if matches!(mode, AddressingMode::Absolute | AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::Indirect) {
             bus.read(pc_after_fetch.wrapping_add(1)) // Read potential second operand byte
        } else { 0x00 };


        // --- Execute instruction ---
        self.program_counter = pc_after_fetch; // Advance PC now
        match opcode {
           // ADC
           0x69 | 0x65 | 0x75 | 0x6D | 0x7D | 0x79 | 0x61 | 0x71 => self.adc(bus, mode),
           // AND
           0x29 | 0x25 | 0x35 | 0x2D | 0x3D | 0x39 | 0x21 | 0x31 => self.and(bus, mode),
           // ASL
           0x0A | 0x06 | 0x16 | 0x0E | 0x1E => self.asl(bus, mode),
           // Branch Instructions
           0x90 => self.bcc(bus, mode), 0xB0 => self.bcs(bus, mode), 0xF0 => self.beq(bus, mode),
           0x30 => self.bmi(bus, mode), 0xD0 => self.bne(bus, mode), 0x10 => self.bpl(bus, mode),
           0x50 => self.bvc(bus, mode), 0x70 => self.bvs(bus, mode),
           // BIT
           0x24 | 0x2C => self.bit(bus, mode),
           // BRK
           0x00 => self.brk(bus, mode),
           // Flag Instructions
           0x18 => self.clc(bus, mode), 0xD8 => self.cld(bus, mode), 0x58 => self.cli(bus, mode),
           0xB8 => self.clv(bus, mode), 0x38 => self.sec(bus, mode), 0xF8 => self.sed(bus, mode),
           0x78 => self.sei(bus, mode),
           // CMP
            0xC9 | 0xC5 | 0xD5 | 0xCD | 0xDD | 0xD9 | 0xC1 | 0xD1 => self.cmp(bus, mode),
            // CPX
            0xE0 | 0xE4 | 0xEC => self.cpx(bus, mode),
            // CPY
            0xC0 | 0xC4 | 0xCC => self.cpy(bus, mode),
            // DEC
            0xC6 | 0xD6 | 0xCE | 0xDE => self.dec(bus, mode),
            // DEX, DEY
            0xCA => self.dex(bus, mode), 0x88 => self.dey(bus, mode),
            // EOR
            0x49 | 0x45 | 0x55 | 0x4D | 0x5D | 0x59 | 0x41 | 0x51 => self.eor(bus, mode),
            // INC
            0xE6 | 0xF6 | 0xEE | 0xFE => self.inc(bus, mode),
            // INX, INY
            0xE8 => self.inx(bus, mode), 0xC8 => self.iny(bus, mode),
            // JMP
            0x4C | 0x6C => self.jmp(bus, mode),
            // JSR
            0x20 => self.jsr(bus, mode),
            // LDA
            0xA9 | 0xA5 | 0xB5 | 0xAD | 0xBD | 0xB9 | 0xA1 | 0xB1 => self.lda(bus, mode),
            // LDX
            0xA2 | 0xA6 | 0xB6 | 0xAE | 0xBE => self.ldx(bus, mode),
            // LDY
            0xA0 | 0xA4 | 0xB4 | 0xAC | 0xBC => self.ldy(bus, mode),
            // LSR
            0x4A | 0x46 | 0x56 | 0x4E | 0x5E => self.lsr(bus, mode),
            // NOP (Official and some unofficial)
            0xEA | 0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA | 0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 | 0x04 | 0x44 | 0x64 | 0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 | 0x0C | 0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => self.nop(bus, mode),
            // ORA
            0x09 | 0x05 | 0x15 | 0x0D | 0x1D | 0x19 | 0x01 | 0x11 => self.ora(bus, mode),
            // Stack Instructions
           0x48 => self.pha(bus, mode), 0x08 => self.php(bus, mode),
           0x68 => self.pla(bus, mode), 0x28 => self.plp(bus, mode),
           // ROL
           0x2A | 0x26 | 0x36 | 0x2E | 0x3E => self.rol(bus, mode),
           // ROR
           0x6A | 0x66 | 0x76 | 0x6E | 0x7E => self.ror(bus, mode),
           // RTI, RTS
           0x40 => self.rti(bus), // RTI doesn't use mode
           0x60 => self.rts(bus), // RTS doesn't use mode
           // SBC (Official + Unofficial 0xEB)
           0xE9 | 0xE5 | 0xF5 | 0xED | 0xFD | 0xF9 | 0xE1 | 0xF1 | 0xEB => self.sbc(bus, mode),
           // STA, STX, STY
           0x85 | 0x95 | 0x8D | 0x9D | 0x99 | 0x81 | 0x91 => self.sta(bus, mode),
           0x86 | 0x96 | 0x8E => self.stx(bus, mode),
           0x84 | 0x94 | 0x8C => self.sty(bus, mode),
           // Transfer Instructions
           0xAA => self.tax(bus, mode), 0xA8 => self.tay(bus, mode),
           0xBA => self.tsx(bus, mode), 0x8A => self.txa(bus, mode),
           0x9A => self.txs(bus, mode), 0x98 => self.tya(bus, mode),

           // Unofficial (Not fully implemented, just avoid panic for now)
           // LAX
           0xA7 | 0xB7 | 0xAF | 0xBF | 0xA3 | 0xB3 => { /* self.lax(bus, mode); */ self.nop(bus, mode); }, // Treat as NOP for now
           // SAX
           0x87 | 0x97 | 0x8F | 0x83 => { /* self.sax(bus, mode); */ self.nop(bus, mode); }, // Treat as NOP for now
            // Add more later if needed (DCP, ISB, SLO, RLA, SRE, RRA)

           _ => panic!("This should not happen if mode match is exhaustive"),
        }

        // --- Calculate actual cycles (TODO: needs refinement) ---
        // For now, just return base_cycles, needs page crossing checks etc.
        let _cycles_executed = base_cycles; // Placeholder

        // --- Log state *after* execution in nestest format ---
        // Format: PC   A  X  Y  P  SP CYC
        // Example: C000  A:00 X:00 Y:00 P:24 SP:FD CYC:  0
        println!(
            "{:04X} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{:>3}", // Note: CYC is PPU cycles in nestest.log, adjust if needed
            pc_before, // PC before instruction fetch
            self.accumulator, // A, X, Y, P, SP after execution
            self.x_register,
            self.y_register,
            self.status,
            self.stack_pointer,
            bus.total_cycles // Ensure this uses the total_cycles from Bus
        );
        // Temporary log with operands for debugging the logger itself
         println!(
             "       Op:{:02X} O1:{:02X} O2:{:02X} Mode:{:?}",
             opcode, operand1, operand2, mode // Log operands and mode for easier debugging
         );

        let extra_cycle1 = 0; // Placeholder for now
        let extra_cycle2 = 0; // Placeholder for now

        let total_cycles_taken = base_cycles + extra_cycle1 + extra_cycle2;

        // --- Add Debug Print Here ---
        println!("[CPU Step End] Op:{:02X} Mode:{:?}, BaseCycles:{}, PageCrossCycle:{}, BranchCycle:{}, TotalCyclesTaken:{}",
                 opcode, mode, base_cycles, extra_cycle1, extra_cycle2, total_cycles_taken);
        // --- End Debug Print ---

        total_cycles_taken // Return the calculated cycles
    }

    // --- Inspection ---
    // Returns a snapshot of the current CPU state
    pub fn inspect(&self) -> InspectState {
        InspectState {
            accumulator: self.accumulator,
            x_register: self.x_register,
            y_register: self.y_register,
            status: self.status,
            program_counter: self.program_counter,
            stack_pointer: self.stack_pointer,
        }
    }

    // pub fn inspect_memory(&self, memory: &Memory, addr: u16) -> u8 {
    //     memory.read(addr)
    // }

    // --- Interrupt Handling ---
    pub fn nmi(&mut self, bus: &mut Bus) {
        println!("CPU: NMIハンドラを実行中...");
        
        // ゼロページアドレス0xD2に値を設定（VBlank検出フラグ）
        bus.write(0xD2, 0x04);
        println!("CPU: NMIハンドラでアドレス0xD2に値0x04を設定しました");
        
        // Push program counter (high byte first) and status to the stack
        self.push(bus, ((self.program_counter >> 8) & 0xFF) as u8);
        self.push(bus, (self.program_counter & 0xFF) as u8);
        // PHP behavior on NMI/IRQ: Break flag is cleared, Unused is set
        let status_with_break = (self.status & !BREAK_FLAG) | UNUSED_FLAG;
        self.push(bus, status_with_break);
        
        // Set the I (interrupt disable) flag
        self.status |= INTERRUPT_DISABLE_FLAG;
        
        // Load the NMI vector from 0xFFFA-0xFFFB
        let low_byte = bus.read(0xFFFA);
        let high_byte = bus.read(0xFFFB);
        self.program_counter = ((high_byte as u16) << 8) | (low_byte as u16);
        
        println!("CPU: NMIハンドラ実行開始, PC={:04X}", self.program_counter);
    }
}

// Default implementation for Cpu6502
impl Default for Cpu6502 {
    fn default() -> Self {
        Self::new()
    }
}

// Removed CpuState struct and its impl block as it was duplicated/replaced by InspectState
