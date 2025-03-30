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

// Registers struct, InspectState struct, AddressingMode enum, etc.
#[derive(Default, Debug, Serialize, Clone)] // Add Serialize
pub struct Registers {
    pub accumulator: u8,
    pub index_x: u8,
    pub index_y: u8,
    pub stack_pointer: u8,
    pub program_counter: u16,
    pub status: u8,
}

#[derive(Serialize, Clone)] // Add Serialize
pub struct InspectState {
    pub registers: Registers,
    pub total_cycles: u64, // Add total cycles if needed
}

#[derive(Debug)]
pub enum AddressingMode { Implied, Accumulator, Immediate, ZeroPage, ZeroPageX, ZeroPageY, Relative, Absolute, AbsoluteX, AbsoluteY, Indirect, IndexedIndirect, IndirectIndexed } // Ensure all modes are defined

// Status flag constants
pub const FLAG_CARRY: u8 = 1 << 0;
pub const FLAG_ZERO: u8 = 1 << 1;
pub const FLAG_INTERRUPT_DISABLE: u8 = 1 << 2;
pub const FLAG_DECIMAL_MODE: u8 = 1 << 3; // Not used in NES
pub const FLAG_BREAK: u8 = 1 << 4;
pub const FLAG_UNUSED: u8 = 1 << 5;
pub const FLAG_OVERFLOW: u8 = 1 << 6;
pub const FLAG_NEGATIVE: u8 = 1 << 7;

// The 6502 CPU core
#[derive(Debug)] // Added Debug derive for easier inspection if needed
pub struct Cpu6502 {
    pub registers: Registers,
    opcode: u8, // Current opcode
}

impl Cpu6502 {
    pub fn new() -> Self {
        Self {
            registers: Registers::default(),
            opcode: 0,
        }
    }

    // Reset the CPU to its initial state - Restored method
    pub fn reset(&mut self, bus: &mut Bus) {
        // Read reset vector
        let lo = bus.read(0xFFFC);
        let hi = bus.read(0xFFFD);
        self.registers.program_counter = u16::from_le_bytes([lo, hi]);

        // Reset registers
        self.registers.stack_pointer = self.registers.stack_pointer.wrapping_sub(3);
        self.registers.status |= FLAG_INTERRUPT_DISABLE; // Set I flag
        // Note: Reset does not affect A, X, Y registers

        // Reset takes 7 cycles (accounted for by Bus)
    }

    // --- Bus Access --- (Renamed from Memory Access)
    // Reads a byte from the bus
    fn read(&self, bus: &mut Bus, addr: u16) -> u8 { // Takes &mut Bus
        // --- DEBUG Zero Page Read ---
        /* // コメントアウト
        if addr == 0x00B5 { // Check if reading from the address being compared in the loop
             println!(
                 "[CPU Read Attempt] Addr: {:04X} (PC: {:04X}, Cycle: {})",
                 addr, self.program_counter, bus.total_cycles
             );
         }
         */
        // --- END DEBUG ---
        bus.read(addr)
    }

    // Writes a byte to the bus
    fn write(&self, bus: &mut Bus, addr: u16, data: u8) {
        // --- DEBUG CPU WRITE --- 
        /* // コメントアウト
        println!(
            "[CPU Write Attempt] Addr: {:04X}, Data: {:02X} (PC: {:04X}, Cycle: {})", 
            addr, data, self.program_counter, bus.total_cycles
        );
        */
        // --- END DEBUG ---
        bus.write(addr, data);
    }

    // Reads a 16-bit word (little-endian) from the bus
    fn read_u16(&self, bus: &mut Bus, addr: u16) -> u16 { // Takes &mut Bus
        let lo = self.read(bus, addr) as u16;
        let hi = self.read(bus, addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    // --- Stack Operations ---
    fn push(&mut self, bus: &mut Bus, value: u8) { // Changed memory to bus
        self.write(bus, 0x0100 + self.registers.stack_pointer as u16, value);
        self.registers.stack_pointer = self.registers.stack_pointer.wrapping_sub(1);
    }

    fn pull(&mut self, bus: &mut Bus) -> u8 { // Already takes &mut Bus
        self.registers.stack_pointer = self.registers.stack_pointer.wrapping_add(1);
        self.read(bus, 0x0100 + self.registers.stack_pointer as u16)
    }

    // --- Flag Updates ---
    fn update_nz_flags(&mut self, value: u8) {
        if value == 0 {
            self.registers.status |= FLAG_ZERO;
        } else {
            self.registers.status &= !FLAG_ZERO;
        }
        if value & FLAG_NEGATIVE != 0 {
            self.registers.status |= FLAG_NEGATIVE;
        } else {
            self.registers.status &= !FLAG_NEGATIVE;
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
                let addr = self.registers.program_counter;
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                addr // Address of the immediate value itself
            }
            AddressingMode::ZeroPage => {
                let addr = self.read(bus, self.registers.program_counter) as u16;
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                addr
            }
            AddressingMode::ZeroPageX => {
                let base = self.read(bus, self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                base.wrapping_add(self.registers.index_x) as u16
            }
            AddressingMode::ZeroPageY => {
                let base = self.read(bus, self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                base.wrapping_add(self.registers.index_y) as u16
            }
            AddressingMode::Absolute => {
                let addr = self.read_u16(bus, self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(2);
                addr
            }
            AddressingMode::AbsoluteX => {
                let base = self.read_u16(bus, self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(2);
                // TODO: Add cycle penalty if page crossed
                base.wrapping_add(self.registers.index_x as u16)
            }
            AddressingMode::AbsoluteY => {
                let base = self.read_u16(bus, self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(2);
                // TODO: Add cycle penalty if page crossed
                base.wrapping_add(self.registers.index_y as u16)
            }
            AddressingMode::Indirect => { // Only used by JMP
                let ptr_addr = self.read_u16(bus, self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(2);
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
                let base = self.read(bus, self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                let ptr = base.wrapping_add(self.registers.index_x);
                let lo = self.read(bus, ptr as u16) as u16;
                let hi = self.read(bus, ptr.wrapping_add(1) as u16) as u16;
                (hi << 8) | lo
            }
            AddressingMode::IndirectIndexed => { // (Indirect), Y
                let base = self.read(bus, self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                let lo = self.read(bus, base as u16) as u16;
                let hi = self.read(bus, base.wrapping_add(1) as u16) as u16;
                let ptr_addr = (hi << 8) | lo;
                // TODO: Add cycle penalty if page crossed
                ptr_addr.wrapping_add(self.registers.index_y as u16)
            }
            AddressingMode::Relative => {
                let offset = self.read(bus, self.registers.program_counter) as i8; // Read offset as signed i8
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                // Calculate target address relative to the *next* instruction's address
                self.registers.program_counter.wrapping_add(offset as u16) // wrapping_add handles signed addition correctly
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
            AddressingMode::Accumulator => self.registers.accumulator,
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
        let carry = (self.registers.status & FLAG_CARRY) as u16;
        let result = self.registers.accumulator as u16 + operand as u16 + carry;

        // Set Carry flag
        if result > 0xFF { self.registers.status |= FLAG_CARRY; } else { self.registers.status &= !FLAG_CARRY; }

        // Set Overflow flag
        if (self.registers.accumulator ^ (result as u8)) & (operand ^ (result as u8)) & FLAG_NEGATIVE != 0 {
            self.registers.status |= FLAG_OVERFLOW;
        } else {
            self.registers.status &= !FLAG_OVERFLOW;
        }

        self.registers.accumulator = result as u8;
        self.update_nz_flags(self.registers.accumulator);
    }

    // AND - Logical AND
    fn and(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.registers.accumulator &= operand;
        self.update_nz_flags(self.registers.accumulator);
    }

    // ASL - Arithmetic Shift Left
    fn asl(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let value = match mode {
            AddressingMode::Accumulator => {
                let acc = self.registers.accumulator;
                self.registers.status = (self.registers.status & !FLAG_CARRY) | ((acc & FLAG_NEGATIVE) >> 7); // Old bit 7 to Carry
                self.registers.accumulator = acc << 1;
                self.registers.accumulator
            }
            _ => {
                let addr = self.get_operand_address(bus, mode); // Pass bus
                let operand = self.read(bus, addr); // Pass bus
                self.registers.status = (self.registers.status & !FLAG_CARRY) | ((operand & FLAG_NEGATIVE) >> 7); // Old bit 7 to Carry
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
            self.registers.program_counter = target_addr;
        } else {
             // Consume the relative offset byte even if branch not taken
             let _offset = self.read(bus, self.registers.program_counter); // Pass bus
             self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
        }
    }

    // BCC - Branch if Carry Clear
    fn bcc(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.registers.status & FLAG_CARRY == 0, mode); // Pass bus
    }
    // BCS - Branch if Carry Set
    fn bcs(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.registers.status & FLAG_CARRY != 0, mode); // Pass bus
    }
    // BEQ - Branch if Equal (Zero flag set)
    fn beq(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.registers.status & FLAG_ZERO != 0, mode); // Pass bus
    }
    // BNE - Branch if Not Equal (Zero flag clear)
    fn bne(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.registers.status & FLAG_ZERO == 0, mode); // Pass bus
    }
    // BMI - Branch if Minus (Negative flag set)
    fn bmi(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.registers.status & FLAG_NEGATIVE != 0, mode); // Pass bus
    }
    // BPL - Branch if Positive (Negative flag clear)
    fn bpl(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.registers.status & FLAG_NEGATIVE == 0, mode); // Pass bus
    }
    // BVC - Branch if Overflow Clear
    fn bvc(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.registers.status & FLAG_OVERFLOW == 0, mode); // Pass bus
    }
    // BVS - Branch if Overflow Set
    fn bvs(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.branch(bus, self.registers.status & FLAG_OVERFLOW != 0, mode); // Pass bus
    }

    // BIT - Test Bits
    fn bit(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        // Zero flag: set if result of A & M is zero
        if self.registers.accumulator & operand == 0 { self.registers.status |= FLAG_ZERO; } else { self.registers.status &= !FLAG_ZERO; }
        // Negative flag: set to bit 7 of M
        if operand & FLAG_NEGATIVE != 0 { self.registers.status |= FLAG_NEGATIVE; } else { self.registers.status &= !FLAG_NEGATIVE; }
        // Overflow flag: set to bit 6 of M
        if operand & FLAG_OVERFLOW != 0 { self.registers.status |= FLAG_OVERFLOW; } else { self.registers.status &= !FLAG_OVERFLOW; }
    }

    // BRK - Force Interrupt
    fn brk(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Signature correct
        self.registers.program_counter = self.registers.program_counter.wrapping_add(1); // BRK has a padding byte
        self.push(bus, (self.registers.program_counter >> 8) as u8); // Pass bus
        self.push(bus, self.registers.program_counter as u8); // Pass bus
        let status_with_break = self.registers.status | FLAG_BREAK | FLAG_UNUSED;
        self.push(bus, status_with_break); // Pass bus
        self.registers.status |= INTERRUPT_DISABLE_FLAG; // Set interrupt disable flag
        self.registers.program_counter = self.read_u16(bus, 0xFFFE); // Load IRQ vector, Pass bus
    }

    // CLC - Clear Carry Flag
    fn clc(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.status &= !FLAG_CARRY;
    }
    // CLD - Clear Decimal Mode Flag (No-op on NES)
    fn cld(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.status &= !DECIMAL_MODE_FLAG;
    }
    // CLI - Clear Interrupt Disable Flag
    fn cli(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.status &= !INTERRUPT_DISABLE_FLAG;
    }
    // CLV - Clear Overflow Flag
    fn clv(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.status &= !FLAG_OVERFLOW;
    }

    // Compare helper (doesn't need bus access)
    fn compare(&mut self, register_value: u8, memory_value: u8) {
        let result = register_value.wrapping_sub(memory_value);
        if register_value >= memory_value { self.registers.status |= FLAG_CARRY; } else { self.registers.status &= !FLAG_CARRY; }
        self.update_nz_flags(result);
    }
    // CMP - Compare Accumulator
    fn cmp(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.compare(self.registers.accumulator, operand);
    }
    // CPX - Compare X Register
    fn cpx(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.compare(self.registers.index_x, operand);
    }
    // CPY - Compare Y Register
    fn cpy(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.compare(self.registers.index_y, operand);
    }

    // DEC - Decrement Memory
    fn dec(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        let value = self.read(bus, addr).wrapping_sub(1); // Pass bus
        self.write(bus, addr, value); // Pass bus
        self.update_nz_flags(value);
    }
    // DEX - Decrement X Register
    fn dex(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.index_x = self.registers.index_x.wrapping_sub(1);
        self.update_nz_flags(self.registers.index_x);
    }
    // DEY - Decrement Y Register
    fn dey(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.index_y = self.registers.index_y.wrapping_sub(1);
        self.update_nz_flags(self.registers.index_y);
    }

    // EOR - Exclusive OR
    fn eor(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.registers.accumulator ^= operand;
        self.update_nz_flags(self.registers.accumulator);
    }

    // INC - Increment Memory
    fn inc(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        let value = self.read(bus, addr).wrapping_add(1); // Pass bus
        self.write(bus, addr, value); // Pass bus
        self.update_nz_flags(value);
    }
    // INX - Increment X Register
    fn inx(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.index_x = self.registers.index_x.wrapping_add(1);
        self.update_nz_flags(self.registers.index_x);
    }
    // INY - Increment Y Register
    fn iny(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.index_y = self.registers.index_y.wrapping_add(1);
        self.update_nz_flags(self.registers.index_y);
    }

    // JMP - Jump
    fn jmp(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        self.registers.program_counter = self.get_operand_address(bus, mode); // Pass bus
    }
    // JSR - Jump to Subroutine
    fn jsr(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let target_addr = self.get_operand_address(bus, mode); // Pass bus
        let return_addr = self.registers.program_counter - 1; // JSR pushes PC-1
        self.push(bus, (return_addr >> 8) as u8); // Pass bus
        self.push(bus, return_addr as u8);       // Pass bus
        self.registers.program_counter = target_addr;
    }

    // LDA - Load Accumulator
    fn lda(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let value = self.fetch_operand(bus, mode); // Pass bus
        self.registers.accumulator = value;
        self.update_nz_flags(self.registers.accumulator);
    }
    // LDX - Load X Register
    fn ldx(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let value = self.fetch_operand(bus, mode); // Pass bus
        self.registers.index_x = value;
        self.update_nz_flags(self.registers.index_x);
    }
    // LDY - Load Y Register
    fn ldy(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let value = self.fetch_operand(bus, mode); // Pass bus
        self.registers.index_y = value;
        self.update_nz_flags(self.registers.index_y);
    }

    // LSR - Logical Shift Right
    fn lsr(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
         let value = match mode {
            AddressingMode::Accumulator => {
                let acc = self.registers.accumulator;
                self.registers.status = (self.registers.status & !FLAG_CARRY) | (acc & FLAG_CARRY); // Old bit 0 to Carry
                self.registers.accumulator = acc >> 1;
                self.registers.accumulator
            }
            _ => {
                let addr = self.get_operand_address(bus, mode); // Pass bus
                let operand = self.read(bus, addr); // Pass bus
                self.registers.status = (self.registers.status & !FLAG_CARRY) | (operand & FLAG_CARRY); // Old bit 0 to Carry
                let result = operand >> 1;
                self.write(bus, addr, result); // Pass bus
                result
            }
        };
        self.update_nz_flags(value); // Negative is always 0 after LSR
        self.registers.status &= !FLAG_NEGATIVE; // Explicitly clear negative flag
    }

    // NOP - No Operation
    fn nop(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        // Do nothing
        // Some unofficial NOPs might consume operands, handle later if needed
    }

    // ORA - Logical Inclusive OR
    fn ora(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let operand = self.fetch_operand(bus, mode); // Pass bus
        self.registers.accumulator |= operand;
        self.update_nz_flags(self.registers.accumulator);
    }

    // PHA - Push Accumulator
    fn pha(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Added mode
        self.push(bus, self.registers.accumulator); // Pass bus
    }
    // PHP - Push Processor Status
    fn php(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Added mode
        // Note: Pushed status has Break and Unused flags set
        let status_with_break = self.registers.status | FLAG_BREAK | FLAG_UNUSED;
        self.push(bus, status_with_break); // Pass bus
    }
    // PLA - Pull Accumulator
    fn pla(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Added mode
        self.registers.accumulator = self.pull(bus); // Pass bus
        self.update_nz_flags(self.registers.accumulator);
    }
    // PLP - Pull Processor Status
    fn plp(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Added mode
        self.registers.status = self.pull(bus); // Pass bus
        self.registers.status &= !FLAG_BREAK; // Break flag is ignored when pulled
        self.registers.status |= FLAG_UNUSED; // Unused flag is always set
    }

    // ROL - Rotate Left
    fn rol(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let carry_in = self.registers.status & FLAG_CARRY;
        let value = match mode {
            AddressingMode::Accumulator => {
                let acc = self.registers.accumulator;
                self.registers.status = (self.registers.status & !FLAG_CARRY) | ((acc & FLAG_NEGATIVE) >> 7); // Old bit 7 to Carry
                self.registers.accumulator = (acc << 1) | carry_in;
                self.registers.accumulator
            }
            _ => {
                let addr = self.get_operand_address(bus, mode); // Pass bus
                let operand = self.read(bus, addr); // Pass bus
                self.registers.status = (self.registers.status & !FLAG_CARRY) | ((operand & FLAG_NEGATIVE) >> 7); // Old bit 7 to Carry
                let result = (operand << 1) | carry_in;
                self.write(bus, addr, result); // Pass bus
                result
            }
        };
        self.update_nz_flags(value);
    }

    // ROR - Rotate Right
    fn ror(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let carry_in = (self.registers.status & FLAG_CARRY) << 7; // Carry to bit 7
         let value = match mode {
            AddressingMode::Accumulator => {
                let acc = self.registers.accumulator;
                self.registers.status = (self.registers.status & !FLAG_CARRY) | (acc & FLAG_CARRY); // Old bit 0 to Carry
                self.registers.accumulator = (acc >> 1) | carry_in;
                self.registers.accumulator
            }
            _ => {
                let addr = self.get_operand_address(bus, mode); // Pass bus
                let operand = self.read(bus, addr); // Pass bus
                self.registers.status = (self.registers.status & !FLAG_CARRY) | (operand & FLAG_CARRY); // Old bit 0 to Carry
                let result = (operand >> 1) | carry_in;
                self.write(bus, addr, result); // Pass bus
                result
            }
        };
        self.update_nz_flags(value);
    }

    // RTI - Return from Interrupt
    fn rti(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Added mode
        self.plp(bus, AddressingMode::Implied); // Pass bus
        let lo = self.pull(bus) as u16; // Pass bus
        let hi = self.pull(bus) as u16; // Pass bus
        self.registers.program_counter = (hi << 8) | lo;
    }

    // RTS - Return from Subroutine
    fn rts(&mut self, bus: &mut Bus, _mode: AddressingMode) { // Added mode
        let lo = self.pull(bus) as u16; // Pass bus
        let hi = self.pull(bus) as u16; // Pass bus
        self.registers.program_counter = ((hi << 8) | lo).wrapping_add(1); // RTS pulls PC-1, add 1
    }

    // SBC - Subtract with Carry
    fn sbc(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        // SBC is effectively ADC with the operand bitwise inverted
        let operand = self.fetch_operand(bus, mode); // Pass bus
        let inverted_operand = !operand;

        let carry = (self.registers.status & FLAG_CARRY) as u16; // Carry acts as NOT borrow
        let result = self.registers.accumulator as u16 + inverted_operand as u16 + carry;

        // Set Carry flag (Set if no borrow occurred, i.e., result >= 0x100)
        if result > 0xFF { self.registers.status |= FLAG_CARRY; } else { self.registers.status &= !FLAG_CARRY; }

        // Set Overflow flag
        if (self.registers.accumulator ^ (result as u8)) & (inverted_operand ^ (result as u8)) & FLAG_NEGATIVE != 0 {
            self.registers.status |= FLAG_OVERFLOW;
        } else {
            self.registers.status &= !FLAG_OVERFLOW;
        }

        self.registers.accumulator = result as u8;
        self.update_nz_flags(self.registers.accumulator);
    }

    // SEC - Set Carry Flag
    fn sec(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.status |= FLAG_CARRY;
    }
    // SED - Set Decimal Mode Flag (No-op on NES)
    fn sed(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.status |= DECIMAL_MODE_FLAG;
    }
    // SEI - Set Interrupt Disable Flag
    fn sei(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.status |= INTERRUPT_DISABLE_FLAG;
    }

    // STA - Store Accumulator
    fn sta(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        self.write(bus, addr, self.registers.accumulator); // Pass bus
    }
    // STX - Store X Register
    fn stx(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        self.write(bus, addr, self.registers.index_x); // Pass bus
    }
    // STY - Store Y Register
    fn sty(&mut self, bus: &mut Bus, mode: AddressingMode) { // Changed memory to bus
        let addr = self.get_operand_address(bus, mode); // Pass bus
        self.write(bus, addr, self.registers.index_y); // Pass bus
    }

    // TAX - Transfer Accumulator to X
    fn tax(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.index_x = self.registers.accumulator;
        self.update_nz_flags(self.registers.index_x);
    }
    // TAY - Transfer Accumulator to Y
    fn tay(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.index_y = self.registers.accumulator;
        self.update_nz_flags(self.registers.index_y);
    }
    // TSX - Transfer Stack Pointer to X
    fn tsx(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.index_x = self.registers.stack_pointer;
        self.update_nz_flags(self.registers.index_x);
    }
    // TXA - Transfer X to Accumulator
    fn txa(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.accumulator = self.registers.index_x;
        self.update_nz_flags(self.registers.accumulator);
    }
    // TXS - Transfer X to Stack Pointer
    fn txs(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.stack_pointer = self.registers.index_x;
        // TXS does not update flags
    }
    // TYA - Transfer Y to Accumulator
    fn tya(&mut self, _bus: &mut Bus, _mode: AddressingMode) { // Changed memory to bus (ignored), added mode
        self.registers.accumulator = self.registers.index_y;
        self.update_nz_flags(self.registers.accumulator);
    }

    // --- Undocumented Opcodes ---
    // Implement later if needed (e.g., LAX, SAX, DCP, ISB, SLO, RLA, SRE, RRA)
    fn lax(&mut self, bus: &mut Bus, mode: AddressingMode) { /* Treat as NOP for now */ self.nop(bus, mode); } // Added mode
    fn sax(&mut self, bus: &mut Bus, mode: AddressingMode) { /* Treat as NOP for now */ self.nop(bus, mode); } // Added mode
    // ... and so on


    // --- Execution Cycle ---
    // Executes a single CPU instruction. Returns the number of cycles consumed.
    pub fn step(&mut self, bus: &mut Bus) -> u8 {
        let pc_before = self.registers.program_counter;
        self.opcode = self.read(bus, pc_before);
        self.registers.program_counter = pc_before.wrapping_add(1);

        // Decode opcode to get addressing mode, base cycles, and instruction function
        // This needs a full lookup table or match statement
        let (mode, base_cycles, instruction_fn) : (AddressingMode, u8, fn(&mut Cpu6502, &mut Bus, AddressingMode)) = match self.opcode {
            // --- Official Opcodes --- 
            // ADC
            0x69 => (AddressingMode::Immediate, 2, Self::adc), 0x65 => (AddressingMode::ZeroPage, 3, Self::adc), 0x75 => (AddressingMode::ZeroPageX, 4, Self::adc),
            0x6D => (AddressingMode::Absolute, 4, Self::adc), 0x7D => (AddressingMode::AbsoluteX, 4, Self::adc), 0x79 => (AddressingMode::AbsoluteY, 4, Self::adc),
            0x61 => (AddressingMode::IndexedIndirect, 6, Self::adc), 0x71 => (AddressingMode::IndirectIndexed, 5, Self::adc),
            // AND
            0x29 => (AddressingMode::Immediate, 2, Self::and), 0x25 => (AddressingMode::ZeroPage, 3, Self::and), 0x35 => (AddressingMode::ZeroPageX, 4, Self::and),
            0x2D => (AddressingMode::Absolute, 4, Self::and), 0x3D => (AddressingMode::AbsoluteX, 4, Self::and), 0x39 => (AddressingMode::AbsoluteY, 4, Self::and),
            0x21 => (AddressingMode::IndexedIndirect, 6, Self::and), 0x31 => (AddressingMode::IndirectIndexed, 5, Self::and),
            // ASL
            0x0A => (AddressingMode::Accumulator, 2, Self::asl), 0x06 => (AddressingMode::ZeroPage, 5, Self::asl), 0x16 => (AddressingMode::ZeroPageX, 6, Self::asl),
            0x0E => (AddressingMode::Absolute, 6, Self::asl), 0x1E => (AddressingMode::AbsoluteX, 7, Self::asl),
            // Branch Instructions
            0x90 => (AddressingMode::Relative, 2, Self::bcc), 0xB0 => (AddressingMode::Relative, 2, Self::bcs), 0xF0 => (AddressingMode::Relative, 2, Self::beq),
            0x30 => (AddressingMode::Relative, 2, Self::bmi), 0xD0 => (AddressingMode::Relative, 2, Self::bne), 0x10 => (AddressingMode::Relative, 2, Self::bpl),
            0x50 => (AddressingMode::Relative, 2, Self::bvc), 0x70 => (AddressingMode::Relative, 2, Self::bvs),
            // BIT
            0x24 => (AddressingMode::ZeroPage, 3, Self::bit), 0x2C => (AddressingMode::Absolute, 4, Self::bit),
            // BRK
            0x00 => (AddressingMode::Implied, 7, Self::brk),
            // Flag Instructions (Implied)
            0x18 => (AddressingMode::Implied, 2, Self::clc), 0xD8 => (AddressingMode::Implied, 2, Self::cld), 0x58 => (AddressingMode::Implied, 2, Self::cli),
            0xB8 => (AddressingMode::Implied, 2, Self::clv), 0x38 => (AddressingMode::Implied, 2, Self::sec), 0xF8 => (AddressingMode::Implied, 2, Self::sed),
            0x78 => (AddressingMode::Implied, 2, Self::sei),
            // CMP
            0xC9 => (AddressingMode::Immediate, 2, Self::cmp), 0xC5 => (AddressingMode::ZeroPage, 3, Self::cmp), 0xD5 => (AddressingMode::ZeroPageX, 4, Self::cmp),
            0xCD => (AddressingMode::Absolute, 4, Self::cmp), 0xDD => (AddressingMode::AbsoluteX, 4, Self::cmp), 0xD9 => (AddressingMode::AbsoluteY, 4, Self::cmp),
            0xC1 => (AddressingMode::IndexedIndirect, 6, Self::cmp), 0xD1 => (AddressingMode::IndirectIndexed, 5, Self::cmp),
            // CPX
            0xE0 => (AddressingMode::Immediate, 2, Self::cpx), 0xE4 => (AddressingMode::ZeroPage, 3, Self::cpx), 0xEC => (AddressingMode::Absolute, 4, Self::cpx),
            // CPY
            0xC0 => (AddressingMode::Immediate, 2, Self::cpy), 0xC4 => (AddressingMode::ZeroPage, 3, Self::cpy), 0xCC => (AddressingMode::Absolute, 4, Self::cpy),
            // DEC
            0xC6 => (AddressingMode::ZeroPage, 5, Self::dec), 0xD6 => (AddressingMode::ZeroPageX, 6, Self::dec), 0xCE => (AddressingMode::Absolute, 6, Self::dec), 0xDE => (AddressingMode::AbsoluteX, 7, Self::dec),
            // DEX, DEY (Implied)
            0xCA => (AddressingMode::Implied, 2, Self::dex), 0x88 => (AddressingMode::Implied, 2, Self::dey),
            // EOR
            0x49 => (AddressingMode::Immediate, 2, Self::eor), 0x45 => (AddressingMode::ZeroPage, 3, Self::eor), 0x55 => (AddressingMode::ZeroPageX, 4, Self::eor),
            0x4D => (AddressingMode::Absolute, 4, Self::eor), 0x5D => (AddressingMode::AbsoluteX, 4, Self::eor), 0x59 => (AddressingMode::AbsoluteY, 4, Self::eor),
            0x41 => (AddressingMode::IndexedIndirect, 6, Self::eor), 0x51 => (AddressingMode::IndirectIndexed, 5, Self::eor),
            // INC
            0xE6 => (AddressingMode::ZeroPage, 5, Self::inc), 0xF6 => (AddressingMode::ZeroPageX, 6, Self::inc), 0xEE => (AddressingMode::Absolute, 6, Self::inc), 0xFE => (AddressingMode::AbsoluteX, 7, Self::inc),
            // INX, INY (Implied)
            0xE8 => (AddressingMode::Implied, 2, Self::inx), 0xC8 => (AddressingMode::Implied, 2, Self::iny),
            // JMP
            0x4C => (AddressingMode::Absolute, 3, Self::jmp), 0x6C => (AddressingMode::Indirect, 5, Self::jmp),
            // JSR
            0x20 => (AddressingMode::Absolute, 6, Self::jsr),
            // LDA
            0xA9 => (AddressingMode::Immediate, 2, Self::lda), 0xA5 => (AddressingMode::ZeroPage, 3, Self::lda), 0xB5 => (AddressingMode::ZeroPageX, 4, Self::lda),
            0xAD => (AddressingMode::Absolute, 4, Self::lda), 0xBD => (AddressingMode::AbsoluteX, 4, Self::lda), 0xB9 => (AddressingMode::AbsoluteY, 4, Self::lda),
            0xA1 => (AddressingMode::IndexedIndirect, 6, Self::lda), 0xB1 => (AddressingMode::IndirectIndexed, 5, Self::lda),
            // LDX
            0xA2 => (AddressingMode::Immediate, 2, Self::ldx), 0xA6 => (AddressingMode::ZeroPage, 3, Self::ldx), 0xB6 => (AddressingMode::ZeroPageY, 4, Self::ldx),
            0xAE => (AddressingMode::Absolute, 4, Self::ldx), 0xBE => (AddressingMode::AbsoluteY, 4, Self::ldx),
            // LDY
            0xA0 => (AddressingMode::Immediate, 2, Self::ldy), 0xA4 => (AddressingMode::ZeroPage, 3, Self::ldy), 0xB4 => (AddressingMode::ZeroPageX, 4, Self::ldy),
            0xAC => (AddressingMode::Absolute, 4, Self::ldy), 0xBC => (AddressingMode::AbsoluteX, 4, Self::ldy),
            // LSR
            0x4A => (AddressingMode::Accumulator, 2, Self::lsr), 0x46 => (AddressingMode::ZeroPage, 5, Self::lsr), 0x56 => (AddressingMode::ZeroPageX, 6, Self::lsr),
            0x4E => (AddressingMode::Absolute, 6, Self::lsr), 0x5E => (AddressingMode::AbsoluteX, 7, Self::lsr),
            // NOP
            0xEA => (AddressingMode::Implied, 2, Self::nop),
            // ORA
            0x09 => (AddressingMode::Immediate, 2, Self::ora), 0x05 => (AddressingMode::ZeroPage, 3, Self::ora), 0x15 => (AddressingMode::ZeroPageX, 4, Self::ora),
            0x0D => (AddressingMode::Absolute, 4, Self::ora), 0x1D => (AddressingMode::AbsoluteX, 4, Self::ora), 0x19 => (AddressingMode::AbsoluteY, 4, Self::ora),
            0x01 => (AddressingMode::IndexedIndirect, 6, Self::ora), 0x11 => (AddressingMode::IndirectIndexed, 5, Self::ora),
            // Stack Instructions (Implied)
           0x48 => (AddressingMode::Implied, 3, Self::pha), 0x08 => (AddressingMode::Implied, 3, Self::php),
           0x68 => (AddressingMode::Implied, 4, Self::pla), 0x28 => (AddressingMode::Implied, 4, Self::plp),
           // ROL
           0x2A => (AddressingMode::Accumulator, 2, Self::rol), 0x26 => (AddressingMode::ZeroPage, 5, Self::rol), 0x36 => (AddressingMode::ZeroPageX, 6, Self::rol),
           0x2E => (AddressingMode::Absolute, 6, Self::rol), 0x3E => (AddressingMode::AbsoluteX, 7, Self::rol),
           // ROR
           0x6A => (AddressingMode::Accumulator, 2, Self::ror), 0x66 => (AddressingMode::ZeroPage, 5, Self::ror), 0x76 => (AddressingMode::ZeroPageX, 6, Self::ror),
           0x6E => (AddressingMode::Absolute, 6, Self::ror), 0x7E => (AddressingMode::AbsoluteX, 7, Self::ror),
           // RTI, RTS (Implied) - Pass mode now
           0x40 => (AddressingMode::Implied, 6, Self::rti),
           0x60 => (AddressingMode::Implied, 6, Self::rts),
           // SBC
           0xE9 => (AddressingMode::Immediate, 2, Self::sbc), 0xE5 => (AddressingMode::ZeroPage, 3, Self::sbc), 0xF5 => (AddressingMode::ZeroPageX, 4, Self::sbc),
           0xED => (AddressingMode::Absolute, 4, Self::sbc), 0xFD => (AddressingMode::AbsoluteX, 4, Self::sbc), 0xF9 => (AddressingMode::AbsoluteY, 4, Self::sbc),
           0xE1 => (AddressingMode::IndexedIndirect, 6, Self::sbc), 0xF1 => (AddressingMode::IndirectIndexed, 5, Self::sbc),
           // STA
           0x85 => (AddressingMode::ZeroPage, 3, Self::sta), 0x95 => (AddressingMode::ZeroPageX, 4, Self::sta),
           0x8D => (AddressingMode::Absolute, 4, Self::sta), 0x9D => (AddressingMode::AbsoluteX, 5, Self::sta), 0x99 => (AddressingMode::AbsoluteY, 5, Self::sta),
           0x81 => (AddressingMode::IndexedIndirect, 6, Self::sta), 0x91 => (AddressingMode::IndirectIndexed, 6, Self::sta),
           // STX
           0x86 => (AddressingMode::ZeroPage, 3, Self::stx), 0x96 => (AddressingMode::ZeroPageY, 4, Self::stx), 0x8E => (AddressingMode::Absolute, 4, Self::stx),
           // STY
           0x84 => (AddressingMode::ZeroPage, 3, Self::sty), 0x94 => (AddressingMode::ZeroPageX, 4, Self::sty), 0x8C => (AddressingMode::Absolute, 4, Self::sty),
           // Transfer Instructions (Implied)
           0xAA => (AddressingMode::Implied, 2, Self::tax), 0xA8 => (AddressingMode::Implied, 2, Self::tay),
           0xBA => (AddressingMode::Implied, 2, Self::tsx), 0x8A => (AddressingMode::Implied, 2, Self::txa),
           0x9A => (AddressingMode::Implied, 2, Self::txs), 0x98 => (AddressingMode::Implied, 2, Self::tya),
           // --- Unofficial Opcodes (Treat as NOP for now) ---
           0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => (AddressingMode::Implied, 2, Self::nop),
           0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => (AddressingMode::Immediate, 2, Self::nop),
           0x04 | 0x44 | 0x64 => (AddressingMode::ZeroPage, 3, Self::nop),
           0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => (AddressingMode::ZeroPageX, 4, Self::nop),
           0x0C => (AddressingMode::Absolute, 4, Self::nop),
           0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => (AddressingMode::AbsoluteX, 4, Self::nop),
           // LAX
           0xA7 => (AddressingMode::ZeroPage, 3, Self::lax), 0xB7 => (AddressingMode::ZeroPageY, 4, Self::lax),
           0xAF => (AddressingMode::Absolute, 4, Self::lax), 0xBF => (AddressingMode::AbsoluteY, 4, Self::lax),
           0xA3 => (AddressingMode::IndexedIndirect, 6, Self::lax), 0xB3 => (AddressingMode::IndirectIndexed, 5, Self::lax),
           // SAX
           0x87 => (AddressingMode::ZeroPage, 3, Self::sax), 0x97 => (AddressingMode::ZeroPageY, 4, Self::sax),
           0x8F => (AddressingMode::Absolute, 4, Self::sax), 0x83 => (AddressingMode::IndexedIndirect, 6, Self::sax),
           // Unofficial SBC
           0xEB => (AddressingMode::Immediate, 2, Self::sbc),
           // Add more later (DCP, ISB, SLO, RLA, SRE, RRA)
           // For now, treat remaining unknowns as NOP (Implied, 2 cycles)
           _ => {
              println!("!!! Treating Unknown Opcode {:02X} as NOP at PC: {:04X} !!!", self.opcode, pc_before);
              (AddressingMode::Implied, 2, Self::nop)
           }
        };

        // Execute the instruction function
        instruction_fn(self, bus, mode);

        // TODO: Calculate extra cycles for page crossing, branches etc.
        let cycles_taken = base_cycles; // Placeholder

        // Logging (Consider moving this to Bus::clock after PPU step)
        println!(
            // Format: PC   OP MNEMONIC?   A  X  Y  P  SP CYC (Simplified format)
            "{:<4X}  {:02X}            A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{:>3}",
            pc_before, self.opcode, // Removed placeholders for mnemonic/bytes
            self.registers.accumulator, self.registers.index_x, self.registers.index_y,
            self.registers.status, self.registers.stack_pointer,
            bus.total_cycles // Bus cycle count *before* PPU step for this instruction
        );

        cycles_taken
    }

    // --- Inspection ---
    // Returns a snapshot of the current CPU state
    pub fn inspect(&self) -> InspectState {
        InspectState {
            registers: self.registers.clone(),
            total_cycles: 0, // Placeholder for now, Bus should provide this
        }
    }

    // pub fn inspect_memory(&self, memory: &Memory, addr: u16) -> u8 {
    //     memory.read(addr)
    // }

    // --- Interrupt Handling ---
    pub fn nmi(&mut self, bus: &mut Bus) {
        let pc = self.registers.program_counter;
        self.push(bus, (pc >> 8) as u8);
        self.push(bus, (pc & 0xFF) as u8);
        let status = (self.registers.status & !FLAG_BREAK) | FLAG_UNUSED;
        self.push(bus, status);
        self.registers.status |= INTERRUPT_DISABLE_FLAG;
        let vector = self.read_u16(bus, 0xFFFA);
        self.registers.program_counter = vector;
        // NMI takes 7 cycles (accounted for by Bus)
    }
}

// Default implementation for Cpu6502
impl Default for Cpu6502 {
    fn default() -> Self {
        Self::new()
    }
}

// Removed CpuState struct and its impl block as it was duplicated/replaced by InspectState
