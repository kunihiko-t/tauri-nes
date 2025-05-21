use serde::Serialize;
use log;
// use crate::bus::Bus; // Use Bus instead of Memory // Keep commented or adjust if Bus is directly used
// use crate::bus::Bus; // ★★★ Use Bus directly ★★★ // Remove direct Bus dependency
use crate::bus::BusAccess; // ★★★ 追加: bus.rs の BusAccess を使用 ★★★
// use crate::debugger::Debugger; // Debugger integration can be added later

// バス操作を表す Enum (削除)
// #[derive(Debug, Clone, PartialEq)]
// pub enum BusAction { ... }

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
#[derive(Debug, Serialize, Clone)] // Defaultを削除
pub struct Registers {
    pub accumulator: u8,
    pub x_register: u8,
    pub y_register: u8,
    pub stack_pointer: u8,
    pub program_counter: u16,
    pub status: u8,
}

// Registerのトレイト実装を上書きして正しい初期値を設定
impl Default for Registers {
    fn default() -> Self {
        Self {
            accumulator: 0,
            x_register: 0,
            y_register: 0,
            stack_pointer: 0xFD, // 初期値を正しく0xFDに設定
            program_counter: 0,
            status: 0x24,       // IRQ disable + unused bit
        }
    }
}

#[derive(Serialize, Clone)] // Add Serialize
pub struct InspectState {
    pub registers: Registers,
    pub total_cycles: u64, // Add total cycles if needed
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AddressingMode {
    Implied,
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
    IndexedIndirect,
    IndirectIndexed,
}

// Status flag constants
pub const FLAG_CARRY: u8 = 1 << 0;
pub const FLAG_ZERO: u8 = 1 << 1;
pub const FLAG_INTERRUPT_DISABLE: u8 = 1 << 2;
pub const FLAG_DECIMAL: u8 = 1 << 3; // NESでは使用されないが、6502互換性のため
pub const FLAG_BREAK: u8 = 1 << 4;   // ソフトウェアBRK命令フラグ
pub const FLAG_UNUSED: u8 = 1 << 5;  // 常に1
pub const FLAG_OVERFLOW: u8 = 1 << 6;
pub const FLAG_NEGATIVE: u8 = 1 << 7;

// The 6502 CPU core
#[derive(Debug, Clone, Serialize)]
pub struct Cpu6502 {
    pub registers: Registers,
    pub cycles: u8,
    nmi_pending: bool,
    brk_executed: bool,
}

// DEBUGフラグの設定
const DEBUG_PRINT: bool = false;

// 命令実行のデバッグログを制限
static mut DEBUG_COUNTER: u32 = 0;
static mut DEBUG_LOG_DONE: bool = false;

// 定数を追加
pub const STACK_BASE: u16 = 0x0100;
pub const STACK_RESET: u8 = 0xFD;
pub const NMI_VECTOR_ADDR: u16 = 0xFFFA;
pub const RESET_VECTOR_ADDR: u16 = 0xFFFC;
pub const IRQ_BRK_VECTOR_ADDR: u16 = 0xFFFE;

impl Cpu6502 {
    pub fn new() -> Self {
        Cpu6502 {
            registers: Registers {
                accumulator: 0,
                x_register: 0,
                y_register: 0,
                stack_pointer: 0xFD,
                program_counter: 0, // Will be set by reset vector
                status: 0b0010_0100, // IRQ disabled, B flag set? Check initial status
            },
            cycles: 0,
            nmi_pending: false,
            brk_executed: false,
        }
    }

    // Reset the CPU state
    pub fn reset(&mut self, bus: &mut impl BusAccess) {
        println!("CPU Reset started...");
        // Fetch the reset vector from memory addresses $FFFC and $FFFD
        let reset_vector = bus.read_u16(RESET_VECTOR_ADDR);
        println!("[CPU Reset] Read reset vector ${:04X} from ${:04X}", reset_vector, RESET_VECTOR_ADDR);
        self.registers.program_counter = reset_vector;

        // Reset registers to initial state
        self.registers.accumulator = 0;
        self.registers.x_register = 0;
        self.registers.y_register = 0;
        self.registers.stack_pointer = STACK_RESET;
        // Set specific flags according to NES documentation (e.g., interrupt disable)
        self.registers.status = FLAG_INTERRUPT_DISABLE | FLAG_UNUSED; // Set unused and IRQ disable flags

        // Reset cycle count. Reset typically takes 8 cycles.
        self.cycles = 8;
        self.nmi_pending = false;
        self.brk_executed = false;
        println!("CPU Reset complete: PC set to ${:04X}, Status: ${:02X}", self.registers.program_counter, self.registers.status);
    }

    // --- Restore Old Bus Access Helpers (if needed, though BusAccess is preferred) ---
    // fn read(&self, bus: &impl BusAccess, addr: u16) -> u8 { bus.read(addr) }
    // fn write(&self, bus: &impl BusAccess, addr: u16, data: u8) { bus.write(addr, data) }
    // fn read_u16(&self, bus: &impl BusAccess, addr: u16) -> u16 { bus.read_u16(addr) }

    // --- Restore Old Stack Helpers ---
    fn push(&mut self, bus: &mut impl BusAccess, data: u8) {
        let addr = 0x0100 + self.registers.stack_pointer as u16;
        bus.write(addr, data);
        self.registers.stack_pointer = self.registers.stack_pointer.wrapping_sub(1);
    }

    fn pull(&mut self, bus: &mut impl BusAccess) -> u8 {
        self.registers.stack_pointer = self.registers.stack_pointer.wrapping_add(1);
        let addr = 0x0100 + self.registers.stack_pointer as u16;
        bus.read(addr)
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

    fn compare(&mut self, reg: u8, operand: u8) {
        let result = reg.wrapping_sub(operand);
        self.update_nz_flags(result);
        if reg >= operand {
            self.registers.status |= FLAG_CARRY;
        } else {
            self.registers.status &= !FLAG_CARRY;
        }
    }

    // --- Restore Original Step Method ---
    pub fn step(&mut self, bus: &mut impl BusAccess) -> u8 {
        // --- Add log BEFORE fetching opcode ---
        // println!("[CPU Step Start] PC=${:04X}", self.registers.program_counter); // Keep this log <-- Remove

        // NMI/IRQ handling... (keep as is)
        if self.nmi_pending {
            // println!("[CPU Step] NMI detected! Handling NMI...");
            self.handle_nmi(bus);
            self.nmi_pending = false;
            return self.cycles;
        }
        let irq_disabled = self.registers.status & FLAG_INTERRUPT_DISABLE != 0;
        if !irq_disabled && self.check_irq(bus) {
            self.handle_irq(bus);
            return self.cycles;
        }

        self.cycles = 0;
        let current_pc = self.registers.program_counter; // Store PC before incrementing

        // オペコードのフェッチ
        let opcode = bus.read(current_pc); // Read from current_pc
        self.registers.program_counter = self.registers.program_counter.wrapping_add(1); // Increment PC *after* read

        // --- Add Debug Print for fetched opcode at target PC --- ★★★ 追加 ★★★
        // if current_pc == 0xF982 { // <-- Remove block
        //      println!("[CPU @ {:04X}] Fetched Opcode {:02X}", current_pc, opcode);
        // }

        // デコードと実行
        let (mode, base_cycles, instr_name) = self.decode_opcode(opcode); // Capture instr_name

        // --- Add Debug Print for decoded instruction at target PC --- ★★★ 追加 ★★★
        // if current_pc == 0xF982 { // <-- Remove block
        //      println!("[CPU @ {:04X}] Decoded: Name={}, Mode={:?}, BaseCycles={}", current_pc, instr_name, mode, base_cycles);
        // }

        let (addr, addr_cycles) = self.calculate_effective_address(bus, mode);

        // --- Add Debug Print before execution at target PC --- ★★★ 追加 ★★★
        // if current_pc == 0xF982 { // <-- Remove block
        //      println!("[CPU @ {:04X}] Executing... Addr=${:04X}, AddrCycles={}", current_pc, addr, addr_cycles);
        // }

        // 命令の実行
        let execution_extra_cycles = self.execute_instruction(bus, opcode, addr, mode, current_pc);
        // println!("[CPU @ {:04X}] Returned from execute_instruction.", current_pc); // ★★★ 追加 ★★★ <-- Remove

        self.cycles += base_cycles + addr_cycles + execution_extra_cycles;
        // println!("[CPU @ {:04X}] Cycles updated.", current_pc); // ★★★ 追加 ★★★ <-- Remove

        // --- Add Debug Print for state AFTER execution (Modify to include F982) ---
        // if current_pc == 0xF982 || current_pc == 0xFA87 { // Include both PCs <-- Remove block
        //      println!("[CPU @ {:04X}] Preparing end log...", current_pc); // ★★★ 追加 ★★★
        //      println!("[CPU @ {:04X} end] Opcode {:02X} ({}) executed. Final PC = {:04X}, Final Status = ${:02X}, Cycles = {}",
        //               current_pc, opcode, instr_name, self.registers.program_counter, self.registers.status, self.cycles);
        // }
        // println!("[CPU @ {:04X}] After end log block.", current_pc); // ★★★ 追加 ★★★ <-- Remove

        // BRK命令の場合はフラグを設定
        if opcode == 0x00 {
            self.brk_executed = true;
        }
         // println!("[CPU @ {:04X}] Before returning cycles.", current_pc); // ★★★ 追加 ★★★ <-- Remove

        self.cycles // Return total cycles for this step
    }

    // IRQが必要かチェックする関数
    fn check_irq(&self, _bus: &impl BusAccess) -> bool {
        // ここでハードウェアIRQ信号をチェックする
        // NESでは通常、マッパーかAPUがIRQを生成
        // 現在は単純に偽を返す
        false
    }

    // IRQ処理を行う関数
    fn handle_irq(&mut self, bus: &mut impl BusAccess) {
        // スタックにレジスタをプッシュ
        self.push(bus, (self.registers.program_counter >> 8) as u8);
        self.push(bus, self.registers.program_counter as u8);

        // Bフラグなしでステータスをプッシュするためにコピー
        let mut status_copy = self.registers.status;
        status_copy &= !FLAG_BREAK; // BRKフラグをクリア
        status_copy |= FLAG_UNUSED; // 未使用フラグをセット
        self.push(bus, status_copy);

        // 割り込み禁止フラグをセット
        self.registers.status |= FLAG_INTERRUPT_DISABLE;

        // IRQベクトルをロード
        self.registers.program_counter = bus.read_u16(IRQ_BRK_VECTOR_ADDR);

        // IRQ処理は7サイクルかかる
        self.cycles = 7;
    }

    // ダミーのデコード関数（実際の命令情報を返す必要がある）
    // TODO: Populate with all opcodes and correct cycle counts / page crossing info
    fn decode_opcode(&self, opcode: u8) -> (AddressingMode, u8, &'static str) {
        match opcode {
            // Official Opcodes (Partial List)
            0x00 => (AddressingMode::Implied, 7, "BRK"),
            0xEA => (AddressingMode::Implied, 2, "NOP"),
            // LDA
            0xA9 => (AddressingMode::Immediate, 2, "LDA"),
            0xA5 => (AddressingMode::ZeroPage, 3, "LDA"),
            0xB5 => (AddressingMode::ZeroPageX, 4, "LDA"),
            0xAD => (AddressingMode::Absolute, 4, "LDA"),
            0xBD => (AddressingMode::AbsoluteX, 4, "LDA"), // +1 cycle if page crossed
            0xB9 => (AddressingMode::AbsoluteY, 4, "LDA"), // +1 cycle if page crossed
            0xA1 => (AddressingMode::IndexedIndirect, 6, "LDA"),
            0xB1 => (AddressingMode::IndirectIndexed, 5, "LDA"), // +1 cycle if page crossed
            // LDX
            0xA2 => (AddressingMode::Immediate, 2, "LDX"),
            0xA6 => (AddressingMode::ZeroPage, 3, "LDX"),
            0xB6 => (AddressingMode::ZeroPageY, 4, "LDX"),
            0xAE => (AddressingMode::Absolute, 4, "LDX"),
            0xBE => (AddressingMode::AbsoluteY, 4, "LDX"), // +1 cycle if page crossed
            // LDY
            0xA0 => (AddressingMode::Immediate, 2, "LDY"),
            0xA4 => (AddressingMode::ZeroPage, 3, "LDY"),
            0xB4 => (AddressingMode::ZeroPageX, 4, "LDY"),
            0xAC => (AddressingMode::Absolute, 4, "LDY"),
            0xBC => (AddressingMode::AbsoluteX, 4, "LDY"), // +1 cycle if page crossed
            // STA
            0x85 => (AddressingMode::ZeroPage, 3, "STA"),
            0x95 => (AddressingMode::ZeroPageX, 4, "STA"),
            0x8D => (AddressingMode::Absolute, 4, "STA"),
            0x9D => (AddressingMode::AbsoluteX, 5, "STA"), // Writes don't add cycle on page cross
            0x99 => (AddressingMode::AbsoluteY, 5, "STA"),
            0x81 => (AddressingMode::IndexedIndirect, 6, "STA"),
            0x91 => (AddressingMode::IndirectIndexed, 6, "STA"),
            // STX
            0x86 => (AddressingMode::ZeroPage, 3, "STX"),
            0x96 => (AddressingMode::ZeroPageY, 4, "STX"),
            0x8E => (AddressingMode::Absolute, 4, "STX"),
            // STY
            0x84 => (AddressingMode::ZeroPage, 3, "STY"),
            0x94 => (AddressingMode::ZeroPageX, 4, "STY"),
            0x8C => (AddressingMode::Absolute, 4, "STY"),
            // JMP
            0x4C => (AddressingMode::Absolute, 3, "JMP"),
            0x6C => (AddressingMode::Indirect, 5, "JMP"),
            // Branches (Base cycles, +1 if taken, +1 if page crossed)
            0x10 => (AddressingMode::Relative, 2, "BPL"), 0x30 => (AddressingMode::Relative, 2, "BMI"),
            0x50 => (AddressingMode::Relative, 2, "BVC"), 0x70 => (AddressingMode::Relative, 2, "BVS"),
            0x90 => (AddressingMode::Relative, 2, "BCC"), 0xB0 => (AddressingMode::Relative, 2, "BCS"),
            0xD0 => (AddressingMode::Relative, 2, "BNE"), 0xF0 => (AddressingMode::Relative, 2, "BEQ"),
            // Other Implied / Accumulator
            0x18 => (AddressingMode::Implied, 2, "CLC"), 0x38 => (AddressingMode::Implied, 2, "SEC"),
            0x58 => (AddressingMode::Implied, 2, "CLI"), 0x78 => (AddressingMode::Implied, 2, "SEI"),
            0xB8 => (AddressingMode::Implied, 2, "CLV"),
            0xD8 => (AddressingMode::Implied, 2, "CLD"), 0xF8 => (AddressingMode::Implied, 2, "SED"),
            0xCA => (AddressingMode::Implied, 2, "DEX"), 0x88 => (AddressingMode::Implied, 2, "DEY"),
            0xE8 => (AddressingMode::Implied, 2, "INX"), 0xC8 => (AddressingMode::Implied, 2, "INY"),
            0xAA => (AddressingMode::Implied, 2, "TAX"), 0xA8 => (AddressingMode::Implied, 2, "TAY"),
            0xBA => (AddressingMode::Implied, 2, "TSX"), 0x8A => (AddressingMode::Implied, 2, "TXA"),
            0x9A => (AddressingMode::Implied, 2, "TXS"), 0x98 => (AddressingMode::Implied, 2, "TYA"),
            // Stack
            0x48 => (AddressingMode::Implied, 3, "PHA"), 0x08 => (AddressingMode::Implied, 3, "PHP"),
            0x68 => (AddressingMode::Implied, 4, "PLA"), 0x28 => (AddressingMode::Implied, 4, "PLP"),
            // JSR / RTS / RTI
            0x20 => (AddressingMode::Absolute, 6, "JSR"),
            0x60 => (AddressingMode::Implied, 6, "RTS"),
            0x40 => (AddressingMode::Implied, 6, "RTI"),
            // Arithmetic & Compare (Immediate)
            0x69 => (AddressingMode::Immediate, 2, "ADC"), 0xE9 => (AddressingMode::Immediate, 2, "SBC"),
            0xC9 => (AddressingMode::Immediate, 2, "CMP"),
            0xE0 => (AddressingMode::Immediate, 2, "CPX"), 0xC0 => (AddressingMode::Immediate, 2, "CPY"),
            // AND, EOR, ORA (Immediate)
            0x29 => (AddressingMode::Immediate, 2, "AND"), 0x49 => (AddressingMode::Immediate, 2, "EOR"),
            0x09 => (AddressingMode::Immediate, 2, "ORA"),
             // BIT
             0x24 => (AddressingMode::ZeroPage, 3, "BIT"),
             0x2C => (AddressingMode::Absolute, 4, "BIT"),
             // ASL, LSR, ROL, ROR (Accumulator & Memory)
             0x0A => (AddressingMode::Accumulator, 2, "ASL"), 0x06 => (AddressingMode::ZeroPage, 5, "ASL"),
             0x16 => (AddressingMode::ZeroPageX, 6, "ASL"), 0x0E => (AddressingMode::Absolute, 6, "ASL"),
             0x1E => (AddressingMode::AbsoluteX, 7, "ASL"),
             0x4A => (AddressingMode::Accumulator, 2, "LSR"), 0x46 => (AddressingMode::ZeroPage, 5, "LSR"),
             0x56 => (AddressingMode::ZeroPageX, 6, "LSR"), 0x4E => (AddressingMode::Absolute, 6, "LSR"),
             0x5E => (AddressingMode::AbsoluteX, 7, "LSR"),
             0x2A => (AddressingMode::Accumulator, 2, "ROL"), 0x26 => (AddressingMode::ZeroPage, 5, "ROL"),
             0x36 => (AddressingMode::ZeroPageX, 6, "ROL"), 0x2E => (AddressingMode::Absolute, 6, "ROL"),
             0x3E => (AddressingMode::AbsoluteX, 7, "ROL"),
             0x6A => (AddressingMode::Accumulator, 2, "ROR"), 0x66 => (AddressingMode::ZeroPage, 5, "ROR"),
             0x76 => (AddressingMode::ZeroPageX, 6, "ROR"), 0x6E => (AddressingMode::Absolute, 6, "ROR"),
             0x7E => (AddressingMode::AbsoluteX, 7, "ROR"),
             // INC, DEC (Memory)
             0xE6 => (AddressingMode::ZeroPage, 5, "INC"), 0xF6 => (AddressingMode::ZeroPageX, 6, "INC"),
             0xEE => (AddressingMode::Absolute, 6, "INC"), 0xFE => (AddressingMode::AbsoluteX, 7, "INC"),
             0xC6 => (AddressingMode::ZeroPage, 5, "DEC"), 0xD6 => (AddressingMode::ZeroPageX, 6, "DEC"),
             0xCE => (AddressingMode::Absolute, 6, "DEC"), 0xDE => (AddressingMode::AbsoluteX, 7, "DEC"),
             // ADC, SBC, CMP (Memory - more modes)
             0x65 => (AddressingMode::ZeroPage, 3, "ADC"), 0x75 => (AddressingMode::ZeroPageX, 4, "ADC"),
             0x6D => (AddressingMode::Absolute, 4, "ADC"), 0x7D => (AddressingMode::AbsoluteX, 4, "ADC"), //+1
             0x79 => (AddressingMode::AbsoluteY, 4, "ADC"), //+1
             0x61 => (AddressingMode::IndexedIndirect, 6, "ADC"), 0x71 => (AddressingMode::IndirectIndexed, 5, "ADC"), //+1
             0xE5 => (AddressingMode::ZeroPage, 3, "SBC"), 0xF5 => (AddressingMode::ZeroPageX, 4, "SBC"),
             0xED => (AddressingMode::Absolute, 4, "SBC"), 0xFD => (AddressingMode::AbsoluteX, 4, "SBC"), //+1
             0xF9 => (AddressingMode::AbsoluteY, 4, "SBC"), //+1
             0xE1 => (AddressingMode::IndexedIndirect, 6, "SBC"), 0xF1 => (AddressingMode::IndirectIndexed, 5, "SBC"), //+1
             0xC5 => (AddressingMode::ZeroPage, 3, "CMP"), 0xD5 => (AddressingMode::ZeroPageX, 4, "CMP"),
             0xCD => (AddressingMode::Absolute, 4, "CMP"), 0xDD => (AddressingMode::AbsoluteX, 4, "CMP"), //+1
             0xD9 => (AddressingMode::AbsoluteY, 4, "CMP"), //+1
             0xC1 => (AddressingMode::IndexedIndirect, 6, "CMP"), 0xD1 => (AddressingMode::IndirectIndexed, 5, "CMP"), //+1
             0xE4 => (AddressingMode::ZeroPage, 3, "CPX"), 0xEC => (AddressingMode::Absolute, 4, "CPX"),
             0xC4 => (AddressingMode::ZeroPage, 3, "CPY"), 0xCC => (AddressingMode::Absolute, 4, "CPY"),
             // AND, ORA, EOR (Memory - more modes)
             0x25 => (AddressingMode::ZeroPage, 3, "AND"), 0x35 => (AddressingMode::ZeroPageX, 4, "AND"),
             0x2D => (AddressingMode::Absolute, 4, "AND"), 0x3D => (AddressingMode::AbsoluteX, 4, "AND"), //+1
             0x39 => (AddressingMode::AbsoluteY, 4, "AND"), //+1
             0x21 => (AddressingMode::IndexedIndirect, 6, "AND"), 0x31 => (AddressingMode::IndirectIndexed, 5, "AND"), //+1
             0x05 => (AddressingMode::ZeroPage, 3, "ORA"), 0x15 => (AddressingMode::ZeroPageX, 4, "ORA"),
             0x0D => (AddressingMode::Absolute, 4, "ORA"), 0x1D => (AddressingMode::AbsoluteX, 4, "ORA"), //+1
             0x19 => (AddressingMode::AbsoluteY, 4, "ORA"), //+1
             0x01 => (AddressingMode::IndexedIndirect, 6, "ORA"), 0x11 => (AddressingMode::IndirectIndexed, 5, "ORA"), //+1
             0x45 => (AddressingMode::ZeroPage, 3, "EOR"), 0x55 => (AddressingMode::ZeroPageX, 4, "EOR"),
             0x4D => (AddressingMode::Absolute, 4, "EOR"), 0x5D => (AddressingMode::AbsoluteX, 4, "EOR"), //+1
             0x59 => (AddressingMode::AbsoluteY, 4, "EOR"), //+1
             0x41 => (AddressingMode::IndexedIndirect, 6, "EOR"), 0x51 => (AddressingMode::IndirectIndexed, 5, "EOR"), //+1

             // --- Unofficial Opcodes (Adding placeholder cycles) ---
             // KIL/HLT/JAM (Treated as NOP for now)
             0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 |
             0x92 | 0xB2 | 0xD2 | 0xF2 => (AddressingMode::Implied, 2, "KIL*"),
             // SLO (ASO) = ASL operand + ORA operand
             0x07 => (AddressingMode::ZeroPage, 5, "SLO*"), 0x17 => (AddressingMode::ZeroPageX, 6, "SLO*"),
             0x0F => (AddressingMode::Absolute, 6, "SLO*"), 0x1F => (AddressingMode::AbsoluteX, 7, "SLO*"),
             0x1B => (AddressingMode::AbsoluteY, 7, "SLO*"),
             0x03 => (AddressingMode::IndexedIndirect, 8, "SLO*"), 0x13 => (AddressingMode::IndirectIndexed, 8, "SLO*"),
             // RLA = ROL operand + AND operand
             0x27 => (AddressingMode::ZeroPage, 5, "RLA*"), 0x37 => (AddressingMode::ZeroPageX, 6, "RLA*"),
             0x2F => (AddressingMode::Absolute, 6, "RLA*"), 0x3F => (AddressingMode::AbsoluteX, 7, "RLA*"),
             0x3B => (AddressingMode::AbsoluteY, 7, "RLA*"),
             0x23 => (AddressingMode::IndexedIndirect, 8, "RLA*"), 0x33 => (AddressingMode::IndirectIndexed, 8, "RLA*"),
             // SRE (LSE) = LSR operand + EOR operand
             0x47 => (AddressingMode::ZeroPage, 5, "SRE*"), 0x57 => (AddressingMode::ZeroPageX, 6, "SRE*"),
             0x4F => (AddressingMode::Absolute, 6, "SRE*"), 0x5F => (AddressingMode::AbsoluteX, 7, "SRE*"),
             0x5B => (AddressingMode::AbsoluteY, 7, "SRE*"),
             0x43 => (AddressingMode::IndexedIndirect, 8, "SRE*"), 0x53 => (AddressingMode::IndirectIndexed, 8, "SRE*"),
             // RRA = ROR operand + ADC operand
             0x67 => (AddressingMode::ZeroPage, 5, "RRA*"), 0x77 => (AddressingMode::ZeroPageX, 6, "RRA*"),
             0x6F => (AddressingMode::Absolute, 6, "RRA*"), 0x7F => (AddressingMode::AbsoluteX, 7, "RRA*"),
             0x7B => (AddressingMode::AbsoluteY, 7, "RRA*"),
             0x63 => (AddressingMode::IndexedIndirect, 8, "RRA*"), 0x73 => (AddressingMode::IndirectIndexed, 8, "RRA*"),
             // SAX (AXS) = Store A & X
             0x87 => (AddressingMode::ZeroPage, 3, "SAX*"), 0x97 => (AddressingMode::ZeroPageY, 4, "SAX*"),
             0x8F => (AddressingMode::Absolute, 4, "SAX*"),
             0x83 => (AddressingMode::IndexedIndirect, 6, "SAX*"),
             // LAX = LDA operand + LDX operand
             0xA7 => (AddressingMode::ZeroPage, 3, "LAX*"), 0xB7 => (AddressingMode::ZeroPageY, 4, "LAX*"),
             0xAF => (AddressingMode::Absolute, 4, "LAX*"), 0xBF => (AddressingMode::AbsoluteY, 4, "LAX*"), //+1
             0xA3 => (AddressingMode::IndexedIndirect, 6, "LAX*"), 0xB3 => (AddressingMode::IndirectIndexed, 5, "LAX*"), //+1
             // DCP (DCM) = DEC operand + CMP operand
             0xC7 => (AddressingMode::ZeroPage, 5, "DCP*"), 0xD7 => (AddressingMode::ZeroPageX, 6, "DCP*"),
             0xCF => (AddressingMode::Absolute, 6, "DCP*"), 0xDF => (AddressingMode::AbsoluteX, 7, "DCP*"),
             0xDB => (AddressingMode::AbsoluteY, 7, "DCP*"),
             0xC3 => (AddressingMode::IndexedIndirect, 8, "DCP*"), 0xD3 => (AddressingMode::IndirectIndexed, 8, "DCP*"),
             // ISC (ISB, INS) = INC operand + SBC operand
             0xE7 => (AddressingMode::ZeroPage, 5, "ISC*"), 0xF7 => (AddressingMode::ZeroPageX, 6, "ISC*"),
             0xEF => (AddressingMode::Absolute, 6, "ISC*"), 0xFF => (AddressingMode::AbsoluteX, 7, "ISC*"),
             0xFB => (AddressingMode::AbsoluteY, 7, "ISC*"),
             0xE3 => (AddressingMode::IndexedIndirect, 8, "ISC*"), 0xF3 => (AddressingMode::IndirectIndexed, 8, "ISC*"),
             // NOPs (unofficial)
             0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => (AddressingMode::Implied, 2, "NOP*"),
             0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => (AddressingMode::Immediate, 2, "NOP*"),
             0x04 | 0x44 | 0x64 => (AddressingMode::ZeroPage, 3, "NOP*"),
             0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => (AddressingMode::ZeroPageX, 4, "NOP*"),
             0x0C => (AddressingMode::Absolute, 4, "NOP*"),
             0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => (AddressingMode::AbsoluteX, 4, "NOP*"), //+1?

             _ => (AddressingMode::Implied, 2, "???"), // Default placeholder for unknown/unimplemented official opcodes
        }
    }

    // --- Restore calculate_effective_address --- ★★★ Fix unused vars ★★★
    fn calculate_effective_address(&mut self, bus: &impl BusAccess, mode: AddressingMode) -> (u16, u8) {
        let mut addr: u16 = 0;
        // Remove unused page_crossed variable
        // let page_crossed = false;
        let mut extra_cycles: u8 = 0; // Renamed for clarity, will be returned

        match mode {
            AddressingMode::Implied | AddressingMode::Accumulator => {}
            AddressingMode::Immediate => {
                addr = self.registers.program_counter;
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
            }
            AddressingMode::ZeroPage => {
                addr = bus.read(self.registers.program_counter) as u16;
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
            }
            AddressingMode::ZeroPageX => {
                let base = bus.read(self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                addr = base.wrapping_add(self.registers.x_register) as u16;
            }
            AddressingMode::ZeroPageY => {
                let base = bus.read(self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                addr = base.wrapping_add(self.registers.y_register) as u16;
            }
            AddressingMode::Absolute => {
                addr = bus.read_u16(self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(2);
            }
            AddressingMode::AbsoluteX => {
                let base = bus.read_u16(self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(2);
                addr = base.wrapping_add(self.registers.x_register as u16);
                if (base & 0xFF00) != (addr & 0xFF00) {
                    extra_cycles = 1; // Set extra_cycles if page crossed
                }
            }
            AddressingMode::AbsoluteY => {
                let base = bus.read_u16(self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(2);
                addr = base.wrapping_add(self.registers.y_register as u16);
                 if (base & 0xFF00) != (addr & 0xFF00) {
                     extra_cycles = 1; // Set extra_cycles if page crossed
                 }
            }
            AddressingMode::Indirect => { // Only used by JMP
                let ptr_addr = bus.read_u16(self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(2);
                // Handle 6502 indirect JMP bug: if the low byte of the address is $FF,
                // the high byte is fetched from $xx00 instead of $xxFF + 1.
                addr = if ptr_addr & 0x00FF == 0x00FF {
                    let lo = bus.read(ptr_addr) as u16;
                    let hi = bus.read(ptr_addr & 0xFF00) as u16; // Read from $xx00
                    (hi << 8) | lo
                } else {
                    bus.read_u16(ptr_addr) // Normal read
                };
            }
            AddressingMode::IndexedIndirect => { // (Indirect, X) - Pre-indexed indirect
                let base_ptr_addr = bus.read(self.registers.program_counter);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                // Address is calculated as (base_ptr_addr + X) % 0x100 (zero page wrap around)
                let ptr_addr = base_ptr_addr.wrapping_add(self.registers.x_register) as u16;
                // Read the effective address from the zero page pointer address
                addr = bus.read_u16_zp(ptr_addr); // Use zero-page wrap-around read for the pointer
            }
            AddressingMode::IndirectIndexed => { // (Indirect), Y - Indirect post-indexed
                let base_ptr_addr = bus.read(self.registers.program_counter) as u16;
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                // Read the base address from the zero page pointer
                let base_addr = bus.read_u16_zp(base_ptr_addr); // Use zero-page wrap-around read for the pointer
                // Add Y register to the base address
                addr = base_addr.wrapping_add(self.registers.y_register as u16);
                 if (base_addr & 0xFF00) != (addr & 0xFF00) {
                    extra_cycles = 1; // Set extra_cycles if page crossed
                 }
            }
            AddressingMode::Relative => { // Branches
                 let offset = bus.read(self.registers.program_counter) as i8;
                 self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                 // Address calculation doesn't add cycles itself, handled by branch logic
                 addr = self.registers.program_counter.wrapping_add(offset as u16);
            }
        }
        (addr, extra_cycles) // Return address and calculated extra cycles
    }

    // --- Execute Instruction (Fix Borrowing) ---
    fn execute_instruction(&mut self, bus: &mut impl BusAccess, opcode: u8, addr: u16, mode: AddressingMode, current_pc: u16) -> u8 {
        let mut _extra_cycles = 0;
        
        // Helper to fetch operand value based on addressing mode
        let operand_value = if mode == AddressingMode::Immediate {
                bus.read(addr)
            } else if mode == AddressingMode::Accumulator {
                self.registers.accumulator
            } else if mode != AddressingMode::Implied && mode != AddressingMode::Relative {
                bus.read(addr) // Read from memory
            } else {
                0 // Implied/Relative doesn't read operand this way
            };
            // --- Side effect triggers moved AFTER instruction logic ---

        match opcode {
            // --- Load Instructions ---
            0xA9 | 0xA5 | 0xB5 | 0xAD | 0xBD | 0xB9 | 0xA1 | 0xB1 => { // LDA
                let value = operand_value;
                self.registers.accumulator = value;
                self.update_nz_flags(value);
            },
            // LDX
            0xA2 | 0xA6 | 0xB6 | 0xAE | 0xBE => { 
                let value = operand_value;
                self.registers.x_register = value;
                self.update_nz_flags(value);
            },
            // LDY
            0xA0 | 0xA4 | 0xB4 | 0xAC | 0xBC => { 
                let value = operand_value;
                self.registers.y_register = value;
                self.update_nz_flags(value);
            },
            
            // --- Store Instructions ---
            0x85 | 0x95 | 0x8D | 0x9D | 0x99 | 0x81 | 0x91 => { // STA
                 if addr == 0x2007 {
                    println!("[CPU STA] Attempting to write to $2007 with Data=${:02X}", self.registers.accumulator);
                 }
                 bus.write(addr, self.registers.accumulator);
            },
            0x86 | 0x96 | 0x8E => { // STX
                 bus.write(addr, self.registers.x_register);
            },
            0x84 | 0x94 | 0x8C => { // STY
                 bus.write(addr, self.registers.y_register);
            },
            
            // --- Transfer Instructions ---
            0xAA => { // TAX
                self.registers.x_register = self.registers.accumulator; 
                self.update_nz_flags(self.registers.x_register);
            },
            0xA8 => { // TAY
                self.registers.y_register = self.registers.accumulator; 
                self.update_nz_flags(self.registers.y_register);
            },
            0xBA => { // TSX
                self.registers.x_register = self.registers.stack_pointer; 
                self.update_nz_flags(self.registers.x_register);
            },
            0x8A => { // TXA
                self.registers.accumulator = self.registers.x_register; 
                self.update_nz_flags(self.registers.accumulator);
            },
            0x9A => { // TXS
                self.registers.stack_pointer = self.registers.x_register;
            },
            0x98 => { // TYA
                self.registers.accumulator = self.registers.y_register; 
                self.update_nz_flags(self.registers.accumulator);
            },
            
            // --- Stack Instructions ---
            0x48 => { // PHA
                self.push(bus, self.registers.accumulator);
            },
            0x08 => { // PHP
                self.push(bus, self.registers.status | FLAG_BREAK | FLAG_UNUSED);
            },
            0x68 => { // PLA
                self.registers.accumulator = self.pull(bus); 
                self.update_nz_flags(self.registers.accumulator);
            },
            0x28 => { // PLP
                self.registers.status = (self.pull(bus) & !FLAG_BREAK) | FLAG_UNUSED;
            },
            
            // --- Increment/Decrement --- (Register only)
            0xE8 => { // INX
                self.registers.x_register = self.registers.x_register.wrapping_add(1); 
                self.update_nz_flags(self.registers.x_register);
            },
            0xC8 => { // INY
                self.registers.y_register = self.registers.y_register.wrapping_add(1); 
                self.update_nz_flags(self.registers.y_register);
            },
            0xCA => { // DEX
                self.registers.x_register = self.registers.x_register.wrapping_sub(1); 
                self.update_nz_flags(self.registers.x_register);
            },
            0x88 => { // DEY
                self.registers.y_register = self.registers.y_register.wrapping_sub(1); 
                self.update_nz_flags(self.registers.y_register);
            },
            
            // --- Increment/Decrement Memory ---
            0xE6 | 0xF6 | 0xEE | 0xFE => { // INC
                let value = operand_value;
                let result = value.wrapping_add(1);
                bus.write(addr, result);
                self.update_nz_flags(result);
            },
            0xC6 | 0xD6 | 0xCE | 0xDE => { // DEC
                let value = operand_value;
                let result = value.wrapping_sub(1);
                bus.write(addr, result);
                self.update_nz_flags(result);
            },
            
            // --- Arithmetic ---
            0x69 | 0x65 | 0x75 | 0x6D | 0x7D | 0x79 | 0x61 | 0x71 => { // ADC
                let value = operand_value;
                self.add(bus, value);
            },
            0xE9 | 0xE5 | 0xF5 | 0xED | 0xFD | 0xF9 | 0xE1 | 0xF1 => { // SBC
                let value = operand_value;
                self.add(bus, !value);
            },
            
            // --- Comparisons ---
            0xC9 | 0xC5 | 0xD5 | 0xCD | 0xDD | 0xD9 | 0xC1 | 0xD1 => { // CMP
                let value = operand_value;
                self.compare(self.registers.accumulator, value);
            },
            0xE0 | 0xE4 | 0xEC => { // CPX
                let value = operand_value;
                self.compare(self.registers.x_register, value);
            },
            0xC0 | 0xC4 | 0xCC => { // CPY
                let value = operand_value;
                self.compare(self.registers.y_register, value);
            },
            
            // --- Logical Operations ---
            0x29 | 0x25 | 0x35 | 0x2D | 0x3D | 0x39 | 0x21 | 0x31 => { // AND
                let value = operand_value;
                self.registers.accumulator &= value;
                self.update_nz_flags(self.registers.accumulator);
            },
            0x09 | 0x05 | 0x15 | 0x0D | 0x1D | 0x19 | 0x01 | 0x11 => { // ORA
                let value = operand_value;
                self.registers.accumulator |= value;
                self.update_nz_flags(self.registers.accumulator);
            },
            0x49 | 0x45 | 0x55 | 0x4D | 0x5D | 0x59 | 0x41 | 0x51 => { // EOR
                let value = operand_value;
                self.registers.accumulator ^= value;
                self.update_nz_flags(self.registers.accumulator);
            },
            
            // --- Bit Operations ---
            0x24 | 0x2C => { // BIT
                let value = operand_value;
                let result = self.registers.accumulator & value;
                
                // Set zero flag based on AND result
                if result == 0 {
                    self.registers.status |= FLAG_ZERO;
                } else {
                    self.registers.status &= !FLAG_ZERO;
                }
                
                // Copy bits 6 and 7 of the value to the status register
                self.registers.status = (self.registers.status & 0x3F) | (value & 0xC0);
            },
            
            // --- Flag Operations ---
            0x18 => self.registers.status &= !FLAG_CARRY, // CLC
            0x38 => self.registers.status |= FLAG_CARRY, // SEC
            0x58 => self.registers.status &= !FLAG_INTERRUPT_DISABLE, // CLI
            0x78 => self.registers.status |= FLAG_INTERRUPT_DISABLE, // SEI
            0xB8 => self.registers.status &= !FLAG_OVERFLOW, // CLV
            0xD8 => self.registers.status &= !FLAG_DECIMAL, // CLD
            0xF8 => self.registers.status |= FLAG_DECIMAL, // SED
            
            // --- Shifts & Rotates ---
            0x0A | 0x06 | 0x16 | 0x0E | 0x1E => { // ASL
                let value = if mode == AddressingMode::Accumulator { 
                    self.registers.accumulator 
                } else { 
                    bus.read(addr) 
                };
                
                // Set carry flag to bit 7
                if (value & 0x80) != 0 {
                    self.registers.status |= FLAG_CARRY;
                } else {
                    self.registers.status &= !FLAG_CARRY;
                }
                
                let result = value << 1;
                self.update_nz_flags(result);
                
                if mode == AddressingMode::Accumulator {
                    self.registers.accumulator = result;
                } else {
                    bus.write(addr, result);
                }
            },
            0x4A | 0x46 | 0x56 | 0x4E | 0x5E => { // LSR
                let value = if mode == AddressingMode::Accumulator { 
                    self.registers.accumulator 
                } else { 
                    bus.read(addr) 
                };
                
                // Set carry flag to bit 0
                if (value & 0x01) != 0 {
                    self.registers.status |= FLAG_CARRY;
                } else {
                    self.registers.status &= !FLAG_CARRY;
                }
                
                let result = value >> 1;
                self.update_nz_flags(result);
                
                if mode == AddressingMode::Accumulator {
                    self.registers.accumulator = result;
                } else {
                    bus.write(addr, result);
                }
            },
            0x2A | 0x26 | 0x36 | 0x2E | 0x3E => { // ROL
                let value = if mode == AddressingMode::Accumulator { 
                    self.registers.accumulator 
                } else { 
                    bus.read(addr) 
                };
                
                let old_carry = if (self.registers.status & FLAG_CARRY) != 0 { 1 } else { 0 };
                
                // Set carry flag to bit 7
                if (value & 0x80) != 0 {
                    self.registers.status |= FLAG_CARRY;
                } else {
                    self.registers.status &= !FLAG_CARRY;
                }
                
                let result = (value << 1) | old_carry;
                self.update_nz_flags(result);
                
                if mode == AddressingMode::Accumulator {
                    self.registers.accumulator = result;
                } else {
                    bus.write(addr, result);
                }
            },
            0x6A | 0x66 | 0x76 | 0x6E | 0x7E => { // ROR
                let value = if mode == AddressingMode::Accumulator { 
                    self.registers.accumulator 
                } else { 
                    bus.read(addr) 
                };
                
                let old_carry = if (self.registers.status & FLAG_CARRY) != 0 { 0x80 } else { 0 };
                
                // Set carry flag to bit 0
                if (value & 0x01) != 0 {
                    self.registers.status |= FLAG_CARRY;
                } else {
                    self.registers.status &= !FLAG_CARRY;
                }
                
                let result = (value >> 1) | old_carry;
                self.update_nz_flags(result);
                
                if mode == AddressingMode::Accumulator {
                    self.registers.accumulator = result;
                } else {
                    bus.write(addr, result);
                }
            },
            
            // --- JMP / JSR ---
            0x4C | 0x6C => self.registers.program_counter = addr, // JMP
            0x20 => { // JSR
                let return_addr = self.registers.program_counter - 1;
                self.push(bus, (return_addr >> 8) as u8);
                self.push(bus, return_addr as u8);
                self.registers.program_counter = addr;
            },
            
            // --- Returns ---
            0x60 => { // RTS
                let lo = self.pull(bus) as u16;
                let hi = self.pull(bus) as u16;
                self.registers.program_counter = ((hi << 8) | lo).wrapping_add(1);
            },
            0x40 => { // RTI
                self.registers.status = self.pull(bus);
                self.registers.status &= !FLAG_BREAK; // Clear B flag
                self.registers.status |= FLAG_UNUSED;  // Set U flag
                let lo = self.pull(bus) as u16;
                let hi = self.pull(bus) as u16;
                self.registers.program_counter = (hi << 8) | lo;
            },
            
            // --- Branches ---
            0x10 | 0x30 | 0x50 | 0x70 | 0x90 | 0xB0 | 0xD0 | 0xF0 => {
                let condition = self.check_branch_condition(opcode);
                if condition {
                    // Branch taken: Add 1 cycle + potential page cross cycle
                    let pc_after_instruction = current_pc.wrapping_add(2); // PC after opcode and operand
                    let page_crossed = self.check_page_cross(pc_after_instruction, addr); // addr is target
                    self.registers.program_counter = addr;
                    return 1 + if page_crossed { 1 } else { 0 };
                } // else: branch not taken, return 0 extra cycles
            },
            
            // --- BRK ---
            0x00 => { // BRK
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                self.push(bus, (self.registers.program_counter >> 8) as u8);
                self.push(bus, (self.registers.program_counter & 0xFF) as u8);
                self.push(bus, self.registers.status | FLAG_BREAK | FLAG_UNUSED);
                self.registers.status |= FLAG_INTERRUPT_DISABLE;
                self.registers.program_counter = bus.read_u16(IRQ_BRK_VECTOR_ADDR);
                self.brk_executed = true;
            },
            
            // --- NOP ---
            0xEA => {}, // NOP - Official NOP
            
            // --- Unofficial NOPs ---
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => {}, // NOPs
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => {}, // NOPs with immediate
            0x04 | 0x44 | 0x64 | 0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => {}, // NOPs with zp
            0x0C | 0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {}, // NOPs with abs
            
            // --- LAX (unofficial) ---
            0xA7 | 0xB7 | 0xAF | 0xBF | 0xA3 | 0xB3 => {
                let value = operand_value;
                self.registers.accumulator = value;
                self.registers.x_register = value;
                self.update_nz_flags(value);
            },
            
            // --- SAX (unofficial) ---
            0x87 | 0x97 | 0x8F | 0x83 => {
                let value = self.registers.accumulator & self.registers.x_register;
                bus.write(addr, value);
            },
            
            // --- Unofficial Opcodes (Treating as NOPs for now, with logging) ---

            // KIL/HLT/JAM (Treated as NOP for now)
            0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 |
            0x92 | 0xB2 | 0xD2 | 0xF2 => {
                println!("WARN: Unofficial KIL/HLT opcode ${:02X} encountered (treated as NOP)", opcode);
                 // Halt emulation? For now, just act as NOP.
            }

            // SLO (ASO) = ASL operand + ORA operand
            0x07 | 0x17 | 0x0F | 0x1F | 0x1B | 0x03 | 0x13 => {
                //println!("WARN: Unofficial SLO/ASO opcode ${:02X} encountered (basic impl)", opcode);
                let operand_val = bus.read(addr);
                // ASL part
                if (operand_val & 0x80) != 0 { self.registers.status |= FLAG_CARRY; } else { self.registers.status &= !FLAG_CARRY; }
                let shifted = operand_val.wrapping_shl(1);
                bus.write(addr, shifted);
                // ORA part
                self.registers.accumulator |= shifted;
                self.update_nz_flags(self.registers.accumulator);
            }

            // RLA = ROL operand + AND operand
            0x27 | 0x37 | 0x2F | 0x3F | 0x3B | 0x23 | 0x33 => {
                //println!("WARN: Unofficial RLA opcode ${:02X} encountered (basic impl)", opcode);
                let operand_val = bus.read(addr);
                let old_carry = self.registers.status & FLAG_CARRY;
                // ROL part
                if (operand_val & 0x80) != 0 { self.registers.status |= FLAG_CARRY; } else { self.registers.status &= !FLAG_CARRY; }
                let rotated = (operand_val << 1) | old_carry;
                bus.write(addr, rotated);
                // AND part
                self.registers.accumulator &= rotated;
                self.update_nz_flags(self.registers.accumulator);
            }

            // SRE (LSE) = LSR operand + EOR operand
            0x47 | 0x57 | 0x4F | 0x5F | 0x5B | 0x43 | 0x53 => {
                println!("WARN: Unofficial SRE/LSE opcode ${:02X} encountered (treated as NOP)", opcode);
                 // Placeholder NOP
            }

            // RRA = ROR operand + ADC operand
            0x67 | 0x77 | 0x6F | 0x7F | 0x7B | 0x63 | 0x73 => {
                println!("WARN: Unofficial RRA opcode ${:02X} encountered (treated as NOP)", opcode);
                 // Placeholder NOP
            }

            // SAX (AXS) = Store A & X
            0x87 | 0x97 | 0x8F | 0x83 => {
                //println!("WARN: Unofficial SAX/AXS opcode ${:02X} encountered (basic impl)", opcode);
                let value = self.registers.accumulator & self.registers.x_register;
                bus.write(addr, value);
            }

            // LAX = LDA operand + LDX operand
            0xA7 | 0xB7 | 0xAF | 0xBF | 0xA3 | 0xB3 => {
                //println!("WARN: Unofficial LAX opcode ${:02X} encountered (basic impl)", opcode);
                let operand_val = bus.read(addr);
                self.registers.accumulator = operand_val;
                self.registers.x_register = operand_val;
                self.update_nz_flags(operand_val);
            }

            // DCP (DCM) = DEC operand + CMP operand
            0xC7 | 0xD7 | 0xCF | 0xDF | 0xDB | 0xC3 | 0xD3 => {
                //println!("WARN: Unofficial DCP/DCM opcode ${:02X} encountered (basic impl)", opcode);
                let operand_val = bus.read(addr).wrapping_sub(1);
                bus.write(addr, operand_val);
                self.compare(self.registers.accumulator, operand_val);
            }

            // ISC (ISB, INS) = INC operand + SBC operand
            0xE7 | 0xF7 | 0xEF | 0xFF | 0xFB | 0xE3 | 0xF3 => {
                //println!("WARN: Unofficial ISC/ISB/INS opcode ${:02X} encountered (basic impl)", opcode);
                 let operand_val = bus.read(addr).wrapping_add(1);
                 bus.write(addr, operand_val);
                 // Reuse SBC logic (effectively A = A + !operand + Carry)
                 let sbc_operand = !operand_val;
                 self.add(bus, sbc_operand);
            }

            // NOPs (unofficial)
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => {}, // NOP (imp)
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => {}, // NOP #i (imm)
            0x04 | 0x44 | 0x64 | 0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => {}, // NOP zp/zp,X
            0x0C | 0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {}, // NOP abs/abs,X

            // --- End Unofficial Opcodes ---

            _ => {
                println!("WARN: Unimplemented or unknown official opcode {:02X} encountered!", opcode);
                // Potentially halt or panic here depending on desired strictness
            }
        }

        // --- Handle Read Side Effects AFTER instruction execution logic ---
        // This prevents side effects from interfering with the instruction's logic
        // (e.g., BIT reading $2002 shouldn't immediately clear the VBlank flag it just read)
        if mode != AddressingMode::Immediate && mode != AddressingMode::Accumulator &&
           mode != AddressingMode::Implied && mode != AddressingMode::Relative {
            
            let effective_addr = addr; // Use the calculated effective address
            
            if (effective_addr & 0xE007) == 0x2002 { // Check for $2002 mirror read
                bus.ppu_status_read_side_effects();
                // Note: The actual value returned to the CPU instruction (operand_value)
                // was read *before* this side effect occurred.
            } else if (effective_addr & 0xE007) == 0x2007 { // Check for $2007 mirror read
                 // $2007 side effects should return the *buffered* value,
                 // but the operand_value already holds the *new* data read for the buffer.
                 // The side effect function handles the buffering logic.
                 bus.ppu_data_read_side_effects(operand_value);
                 // The CPU instruction already used the value from the buffer (implicitly handled by ppu_data_read_side_effects before?) - this needs review.
                 // For now, we assume the original read got the buffered value, and this call just updates the buffer.
            }
        }
        // Default: No extra cycles from execution itself
        0
    }

    // --- check_branch_condition (needs opcode argument) ---
    fn check_branch_condition(&self, opcode: u8) -> bool {
        match opcode {
            0x10 => (self.registers.status & FLAG_NEGATIVE) == 0, // BPL
            0x30 => (self.registers.status & FLAG_NEGATIVE) != 0, // BMI
            0x50 => (self.registers.status & FLAG_OVERFLOW) == 0, // BVC
            0x70 => (self.registers.status & FLAG_OVERFLOW) != 0, // BVS
            0x90 => (self.registers.status & FLAG_CARRY) == 0,    // BCC
            0xB0 => (self.registers.status & FLAG_CARRY) != 0,    // BCS
            0xD0 => (self.registers.status & FLAG_ZERO) == 0,    // BNE
            0xF0 => (self.registers.status & FLAG_ZERO) != 0,    // BEQ
            _ => false,
        }
    }

    // --- check_page_cross --- (remains the same) ---
     fn check_page_cross(&self, addr1: u16, addr2: u16) -> bool {
         (addr1 & 0xFF00) != (addr2 & 0xFF00)
     }

    // --- Inspection ---
    pub fn inspect(&self) -> InspectState {
        InspectState {
            registers: self.registers.clone(),
            total_cycles: 0, // Placeholder for now, Bus should provide this
        }
    }

    // --- Interrupt Handling ---
    // NMI割り込み処理 - handle_nmiメソッドの追加
    fn handle_nmi(&mut self, bus: &mut impl BusAccess) -> u8 {
        // PCをスタックにプッシュ
        self.push(bus, (self.registers.program_counter >> 8) as u8);
        self.push(bus, (self.registers.program_counter & 0xFF) as u8);
        
        // ステータスレジスタをスタックにプッシュ (Bフラグをクリア、UNUSEDフラグをセット)
        self.push(bus, (self.registers.status & !FLAG_BREAK) | FLAG_UNUSED);
        
        // 割り込み禁止フラグをセット
        self.registers.status |= FLAG_INTERRUPT_DISABLE;
        
        // NMIベクターからPCを読み込む
        self.registers.program_counter = bus.read_u16(NMI_VECTOR_ADDR);
        
        // NMIには7サイクルかかる
        self.cycles = 7;
        
        if DEBUG_PRINT {
            println!("NMI triggered! PC set to ${:04X}", self.registers.program_counter);
        }
        7
    }

    // is_brk_executedメソッドの修正 - opcodeを使用せずにbrk_executedフラグを使う
    pub fn is_brk_executed(&self) -> bool {
        self.brk_executed
    }

    // trigger_nmiメソッドの修正
    pub fn trigger_nmi(&mut self) {
        self.nmi_pending = true;
    }

    // --- Other Debug Helpers ---
    pub fn debug_stack_pointer(&self) -> String {
        format!("SP=${:02X} (Points to ${:04X})",
                 self.registers.stack_pointer, 0x0100 + self.registers.stack_pointer as u16)
    }
    // pub fn dump_memory(&self, bus: &Bus, ...) { ... } // Needs &Bus again -> Requires BusAccess
    pub fn dump_memory(&self, bus: &impl BusAccess, start_addr: u16, length: u16) {
         println!("Memory Dump from ${:04X} to ${:04X}:", start_addr, start_addr.saturating_add(length).saturating_sub(1));
         let effective_length = length.min(256); // Limit dump length

         for base_addr_offset in (0..effective_length).step_by(16) {
             let base_addr = match start_addr.checked_add(base_addr_offset) {
                 Some(addr) => addr,
                 None => break,
             };
             if base_addr >= start_addr.saturating_add(length) && length > 0 { break; }

             print!("${:04X}:", base_addr);
             let mut bytes_line = String::new();
             let mut ascii_line = String::new();

             for i in 0..16 {
                 let current_addr = match base_addr.checked_add(i) {
                     Some(addr) => addr,
                     None => break,
                 };
                 if current_addr >= start_addr.saturating_add(length) {
                      bytes_line.push_str("   ");
                      ascii_line.push(' ');
                 } else {
                     let byte = bus.read(current_addr); // Use bus.read
                     bytes_line.push_str(&format!(" {:02X}", byte));
                     ascii_line.push(if (0x20..=0x7E).contains(&byte) { byte as char } else { '.' });
                 }
             }
             println!("{}  |{}|", bytes_line, ascii_line);
         }
     }

    pub fn add(&mut self, bus: &impl BusAccess, operand: u8) {
        let acc = self.registers.accumulator;
        let carry = self.registers.status & FLAG_CARRY;

        let sum16 = acc as u16 + operand as u16 + carry as u16;
        let result = sum16 as u8;

        // Set Carry flag
        if sum16 > 0xFF {
            self.registers.status |= FLAG_CARRY;
        } else {
            self.registers.status &= !FLAG_CARRY;
        }

        // Set Overflow flag
        // Overflow = (A^operand) & 0x80 == 0 && (A^result) & 0x80 != 0
        if ((acc ^ operand) & 0x80 == 0) && ((acc ^ result) & 0x80 != 0) {
            self.registers.status |= FLAG_OVERFLOW;
        } else {
            self.registers.status &= !FLAG_OVERFLOW;
        }

        self.registers.accumulator = result;
        self.update_nz_flags(result);
    }

    pub fn branch(&mut self, offset: i8) -> u8 {
        let old_pc = self.registers.program_counter;
        
        // Convert PC to i32 to handle potential overflow during addition
        let pc = self.registers.program_counter as i32;
        let new_pc = (pc + offset as i32) & 0xFFFF;
        self.registers.program_counter = new_pc as u16;
        
        // Check if page boundary crossed (high byte changed)
        let page_crossed = (old_pc & 0xFF00) != (self.registers.program_counter & 0xFF00);
        
        if page_crossed {
            2 // Additional cycle for page boundary crossing
        } else {
            1 // Branch taken without page crossing
        }
    }
}

// --- Default Trait Implementation ---
impl Default for Cpu6502 {
    fn default() -> Self {
        Self::new()
    }
}

// ★★★ Add decode_for_disassembly (basic version) ★★★
impl Cpu6502 {
     pub fn decode_for_disassembly(&self, opcode: u8) -> (&'static str, u8, &'static str) {
         // Use the existing decode_opcode but extract relevant parts
         let (mode, _, name) = self.decode_opcode(opcode);
         let operand_bytes = match mode {
             AddressingMode::Implied | AddressingMode::Accumulator => 0,
             AddressingMode::Immediate | AddressingMode::ZeroPage | AddressingMode::ZeroPageX |
             AddressingMode::ZeroPageY | AddressingMode::Relative | AddressingMode::IndexedIndirect |
             AddressingMode::IndirectIndexed => 1,
             AddressingMode::Absolute | AddressingMode::AbsoluteX | AddressingMode::AbsoluteY |
             AddressingMode::Indirect => 2,
         };
         let mode_str = match mode {
             AddressingMode::Implied => "Implied",
             AddressingMode::Accumulator => "Accumulator",
             AddressingMode::Immediate => "Immediate",
             AddressingMode::ZeroPage => "Zero Page",
             AddressingMode::ZeroPageX => "Zero Page, X",
             AddressingMode::ZeroPageY => "Zero Page, Y",
             AddressingMode::Relative => "Relative",
             AddressingMode::Absolute => "Absolute",
             AddressingMode::AbsoluteX => "Absolute, X",
             AddressingMode::AbsoluteY => "Absolute, Y",
             AddressingMode::Indirect => "Indirect",
             AddressingMode::IndexedIndirect => "(Indirect, X)",
             AddressingMode::IndirectIndexed => "(Indirect), Y",
         };
         (name, operand_bytes, mode_str)
     }
}
