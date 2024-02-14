use serde::Serialize;

use crate::ram::Memory;
use crate::debugger::Debugger;

const CARRY_FLAG: u8 = 0b00000001;
const ZERO_FLAG: u8 = 0b00000010;
const INTERRUPT_DISABLE_FLAG: u8 = 0b00000100;
const DECIMAL_MODE_FLAG: u8 = 0b00001000;
const BREAK_FLAG: u8 = 0b00010000;
const UNUSED_FLAG: u8 = 0b00100000;
const OVERFLOW_FLAG: u8 = 0b01000000;
const NEGATIVE_FLAG: u8 = 0b10000000;
// 6502 CPUの状態を保持する構造体
#[derive(Serialize)]
pub struct Cpu6502 {
    accumulator: u8,
    x_register: u8,
    y_register: u8,
    status: u8,
    program_counter: u16,
    stack_pointer: u8,
    memory: Memory,
}


// 6502 CPUの状態を保持する構造体
#[derive(Serialize)]
#[derive(Clone)]
pub struct CpuState {
    accumulator: u8,
    x_register: u8,
    y_register: u8,
    status: u8,
    program_counter: u16,
    // 他のレジスタやフラグがあればここに追加
}


pub struct Registers {
    accumulator: u8,
    x_index: u8,
    y_index: u8,
    status: u8,
    program_counter: u16,
    stack_pointer: u8,
}

impl Registers {
    fn new() -> Self {
        Self {
            accumulator: 0,
            x_index: 0,
            y_index: 0,
            status: 0,
            program_counter: 0,
            stack_pointer: 0,
        }
    }
}


enum AddressingMode {
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Indirect,
    IndexedIndirect,
    IndirectIndexed,
    Relative,
    Accumulator,
    AbsoluteIndexedX,
    AbsoluteIndexedY,
}

impl Cpu6502 {
    pub(crate) fn new() -> Self {
        Self {
            accumulator: 0,
            x_register: 0,
            y_register: 0,
            status: 0x24, // Set unused and break flags
            program_counter: 0xC000, // Common start address for NES
            stack_pointer: 0,
            memory: Memory::new()
        }
    }
    fn rol(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        let carry_in = (self.status & CARRY_FLAG) << 7;  // Move current carry flag to bit 7
        let new_carry = value & 0x80;  // Old bit 7 becomes new carry.
        let value = ((value << 1) & 0xFE) | carry_in;  // Shift left (bit 0 empty), then new carry in.
        self.write(addr, value);
        // Update status register: Set carry flag if new_carry != 0, otherwise clear carry flag.
        self.status = if new_carry != 0 { self.status | CARRY_FLAG } else { self.status & !CARRY_FLAG };
        // Also update zero and negative flags
        self.update_zero_and_negative_flags(value);
    }

    fn ror(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        let carry_in = (self.status & CARRY_FLAG) << 7;  // Move current carry flag to bit 7
        let new_carry = value & 0x01;  // Old bit 0 becomes new carry.
        let value = ((value >> 1) & 0x7F) | carry_in;  // Shift right (bit 7 empty), then new carry in.
        self.write(addr, value);
        // Update status register: Set carry flag if new_carry != 0, otherwise clear carry flag.
        self.status = if new_carry != 0 { self.status | CARRY_FLAG } else { self.status & !CARRY_FLAG };
        // Also update zero and negative flags
        self.update_zero_and_negative_flags(value);
    }

    fn clc(&mut self, _mode: AddressingMode) {
        self.status &= 0xFE;  // Clear carry flag
    }

    fn ldx(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.x_register = value;
        self.update_zero_and_negative_flags(self.x_register);
    }

    fn stx(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.write(addr, self.x_register);
    }

    fn dex(&mut self, _mode: AddressingMode) {
        self.x_register = self.x_register.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.x_register);
    }

    fn beq(&mut self, mode: AddressingMode) {
        if self.status & ZERO_FLAG != 0 {
            self.program_counter = self.get_operand_address(mode);
        } else {
            self.program_counter += 2;
        }
        // This instruction adds 1 to PC if branch occurs on the same page and adds 2 otherwise.
    }

    fn bmi(&mut self, mode: AddressingMode) {
        if self.status & NEGATIVE_FLAG != 0 {
            self.program_counter = self.get_operand_address(mode);
        } else {
            self.program_counter += 2;
        }
    }
    fn jsr(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.push_stack((self.program_counter >> 8) as u8); // push high byte of PC to stack
        self.push_stack(self.program_counter as u8); // push low byte of PC to stack
        self.program_counter = addr;
    }

    fn rts(&mut self, _mode: AddressingMode) { // Note: this instruction uses "implied" mode
        let low_byte = self.pull_stack() as u16;
        let high_byte = self.pull_stack() as u16;
        self.program_counter = (high_byte << 8) | low_byte;
        self.program_counter += 1;
    }

    fn bne(&mut self, mode: AddressingMode) {
        if self.status & ZERO_FLAG == 0 {
            self.program_counter = self.get_operand_address(mode);
        } else {
            self.program_counter += 2;
        }
        //This instruction adds 1 to program_counter if branch occurs on the same page, or adds 2 if it occurs on a different page.
    }

    fn ldy(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.y_register = value;
        self.update_zero_and_negative_flags(self.y_register);
    }

    fn sty(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.write(addr, self.y_register);
    }

    fn dey(&mut self, _mode: AddressingMode) {
        self.y_register = self.y_register.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.y_register);
    }
    fn get_operand_address_abs(&mut self) -> u16 {
        let low_byte = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        let high_byte = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        (high_byte << 8) | low_byte
    }
    fn get_operand_address(&mut self, mode: AddressingMode) -> u16 {
        match mode {
            AddressingMode::Immediate => {
                let addr = self.program_counter;
                self.program_counter += 1;
                addr
            },
            AddressingMode::ZeroPage => {
                let addr = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                addr
            },
            AddressingMode::Absolute => {
                let low = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                let high = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                (high << 8) | low
            },
            AddressingMode::ZeroPageX => {
                let zero_page_addr = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                (zero_page_addr.wrapping_add(self.x_register as u16)) & 0x00FF // Zero page wrap around
            },
            AddressingMode::ZeroPageY => {
                let zero_page_addr = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                (zero_page_addr.wrapping_add(self.y_register as u16)) & 0x00FF // Zero page wrap around
            },
            AddressingMode::AbsoluteX => {
                let low = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                let high = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                ((high << 8) | low).wrapping_add(self.x_register as u16) // No page-boundary wrap in NES
            },
            AddressingMode::AbsoluteY => {
                let low = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                let high = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                ((high << 8) | low).wrapping_add(self.y_register as u16) // No page-boundary wrap in NES
            },
            AddressingMode::Indirect => {
                let low_addr = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                let high_addr = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                let addr = (high_addr << 8) | low_addr;
                (self.read(addr + 1) as u16) << 8 | self.read(addr) as u16
            },
            AddressingMode::IndexedIndirect => {
                let zero_page_addr = (self.read(self.program_counter).wrapping_add(self.x_register)) as u16;
                self.program_counter += 1;
                (self.read((zero_page_addr + 1) & 0x00FF) as u16) << 8 | self.read(zero_page_addr & 0x00FF) as u16
            },
            AddressingMode::IndirectIndexed => {
                let zero_page_addr = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                ((self.read((zero_page_addr + 1) & 0x00FF).wrapping_add(self.y_register) as u16) << 8) | self.read(zero_page_addr & 0x00FF) as u16
            },
            AddressingMode::Relative => {
                let offset = self.read(self.program_counter) as u16;
                self.program_counter += 1;
                if offset & 0x80 > 0 {
                    self.program_counter.wrapping_sub((offset ^ 0xFF) + 1) // `^ 0xFF` is a way to get two's complement for subtraction
                } else {
                    self.program_counter.wrapping_add(offset)
                }
            },
            AddressingMode::Accumulator => {
                self.accumulator
            },
            AddressingMode::AbsoluteIndexedX => {
                let base_addr = self.get_operand_address_abs();
                let addr = base_addr.wrapping_add(self.x_register as u16);
                addr
            },
            AddressingMode::AbsoluteIndexedY => {
                let base_addr = self.get_operand_address_abs();
                let addr = base_addr.wrapping_add(self.y_register as u16);
                addr
            },
            // 他のアドレッシングモードに対する実装...
            _ => unimplemented!("Addressing mode not implemented"),
        }
    }

    fn update_nz_flags(&mut self, value: u8) {
        if value == 0 {
            self.status |= ZERO_FLAG;
        } else {
            self.status &= !ZERO_FLAG;
        }
        if value & 0x80 > 0 {
            self.status |= NEGATIVE_FLAG;
        } else {
            self.status &= !NEGATIVE_FLAG;
        }
    }

    fn sbc(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        let carry = if self.status & CARRY_FLAG > 0 { 0 } else { 1 };
        let (part_res, overflow1) = self.accumulator.overflowing_sub(value);
        let (res, overflow2) = part_res.overflowing_sub(carry);
        self.update_nz_flags(res);
        self.accumulator = res;
        self.status = if overflow1 || overflow2 { self.status | CARRY_FLAG } else { self.status & !CARRY_FLAG };
    }

    fn inc(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        let res = value.wrapping_add(1);
        self.write(addr, res);
        self.update_nz_flags(res);
    }

    fn inx(&mut self, _mode: AddressingMode) {
        self.x_register = self.x_register.wrapping_add(1);
        self.update_nz_flags(self.x_register);
    }

    fn iny(&mut self, _mode: AddressingMode) {
        self.y_register = self.y_register.wrapping_add(1);
        self.update_nz_flags(self.y_register);
    }
    fn cmp(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.execute_subtraction(self.accumulator, value);
    }

    fn cpx(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.execute_subtraction(self.x_register, value);
    }

    fn cpy(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.execute_subtraction(self.y_register, value);
    }

    fn execute_subtraction(&mut self, reg_value: u8, value: u8) {
        let (res, carry) = reg_value.overflowing_sub(value);
        if carry {
            self.status &= !CARRY_FLAG;
        } else {
            self.status |= CARRY_FLAG;
        }
        self.update_zero_and_negative_flags(res);
    }

    fn adc(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let mem_val = self.read(addr);
        let (res, overflow) = self.accumulator.overflowing_add(mem_val);

        if overflow || self.status & CARRY_FLAG > 0 {
            self.status |= CARRY_FLAG;
        }

        self.accumulator = res;
        self.update_zero_and_negative_flags(self.accumulator);
    }

    fn and(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let mem_val = self.read(addr);
        self.accumulator &= mem_val;
        self.update_zero_and_negative_flags(self.accumulator);
    }

    fn asl(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let mem_val = self.read(addr);
        self.status = (self.status & 0xFE) | (mem_val >> 7);
        let res = mem_val << 1;
        self.write(addr, res);
        self.update_zero_and_negative_flags(res);
    }

    fn bcc(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        if self.status & CARRY_FLAG == 0 {
            self.program_counter = addr;
        }
    }

    pub fn step(&mut self, memory: &mut Memory, debugger: &Debugger) {
        if debugger.check_breakpoint(self.program_counter) {
            // ブレークポイントに達した場合の処理
        }
        self.execute();
    }
}

// 6502 CPUの命令セットを実装する
impl Cpu6502 {

    fn lda(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.accumulator = value;
        self.update_zero_and_negative_flags(value);
    }

    fn ldx(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.x_register = value;
        self.update_zero_and_negative_flags(value);
    }

    fn ldy(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.y_register = value;
        self.update_zero_and_negative_flags(value);
    }

    // STA: Store Accumulator
    fn sta(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.write(addr, self.accumulator);
    }

    fn stx(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.write(addr, self.x_register);
    }

    fn sty(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.write(addr, self.y_register);
    }



    // BEQ: Branch if Equal
    fn beq(&mut self, offset: i8) {
        if self.status & 0x02 != 0 {  // ゼロフラグがセットされている場合
            self.program_counter = ((self.program_counter as i16) + offset as i16) as u16;
        }
    }


    // 即値（Immediate）アドレッシング
    fn immediate(&mut self) -> u8 {
        let value = self.read(self.program_counter);
        self.program_counter += 1;
        value
    }

    // ゼロページ（Zero Page）アドレッシング
    fn zero_page(&mut self) -> u16 {
        let addr = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        addr
    }



    // 絶対（Absolute）アドレッシング
    fn absolute(&mut self) -> u16 {
        let low = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        let high = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        (high << 8) | low
    }

    fn and(&mut self, mode: AddressingMode) {
        // AND命令の実装
    }

    fn ora(&mut self, mode: AddressingMode) {
        // ORA命令の実装
    }

    fn adc(&mut self, mode: AddressingMode) {
        // ADC命令の実装
    }

    fn sbc(&mut self, mode: AddressingMode) {
        // SBC命令の実装
    }


    // メモリから値を読み取る
    fn read(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }

    // メモリに値を書き込む
    fn write(&mut self, addr: u16, data: u8) {
        self.memory.write(addr, data);
    }

    fn jmp(&mut self, addr: u16) {
        self.program_counter = addr;
    }

    pub(crate) fn get_status(&self) -> CpuState {
        CpuState {
            accumulator: self.accumulator,
            x_register: self.x_register,
            y_register: self.y_register,
            status: self.status,
            program_counter: self.program_counter,
        }
    }


    // フラグの更新
    fn update_zero_and_negative_flags(&mut self, value: u8) {
        self.status = (self.status & !(ZERO_FLAG | NEGATIVE_FLAG))
            | ((value == 0) as u8 * ZERO_FLAG)
            | (((value & 0x80) != 0) as u8 * NEGATIVE_FLAG);
    }

    fn eor(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.accumulator ^= value;
        self.update_zero_and_negative_flags(self.accumulator);
    }

    fn dec(&mut self, mode: AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.read(addr);
        self.write(addr, value.wrapping_sub(1));
        self.update_zero_and_negative_flags(value.wrapping_sub(1));
    }

    fn sec(&mut self, _mode: AddressingMode) { // Note: SEC is implied mode, the mode parameter will be ignored.
        self.status |= CARRY_FLAG;
    }

    // ADC (Add with Carry) Immediate
    fn adc_immediate(&mut self) {
        let value = self.read(self.program_counter);
        self.program_counter += 1;
        // ADCの実装
    }

    // AND (Logical AND) Zero Page
    fn and_zero_page(&mut self) {
        let addr = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        let value = self.read(addr);
        // ANDの実装
    }

    // ASL (Arithmetic Shift Left) Accumulator
    fn asl_accumulator(&mut self) {
        // ASLの実装
    }

    // BCC (Branch if Carry Clear) Relative
    fn bcc_relative(&mut self) {
        let offset = self.read(self.program_counter) as i8;
        self.program_counter += 1;
        if self.status & CARRY_FLAG == 0 {
            self.program_counter = ((self.program_counter as i16) + offset as i16) as u16;
        }
    }

    // CMP (Compare Accumulator) Immediate
    fn cmp_immediate(&mut self) {
        let value = self.read(self.program_counter);
        self.program_counter += 1;
        // CMPの実装
    }

    // DEC (Decrement Memory) Zero Page
    fn dec_zero_page(&mut self) {
        let addr = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        let value = self.read(addr).wrapping_sub(1);
        self.write(addr, value);
        // DECの実装
    }

    // EOR (Exclusive OR) Absolute
    fn eor_absolute(&mut self) {
        let addr = self.read_word(self.program_counter);
        self.program_counter += 2;
        let value = self.read(addr);
        // EORの実装
    }

    // LDX (Load X Register) Immediate
    fn ldx_immediate(&mut self) {
        let value = self.read(self.program_counter);
        self.program_counter += 1;
        self.x_register = value;
        // LDXの実装
    }

    // 16ビットのアドレスを読み取る
    fn read_word(&self, addr: u16) -> u16 {
        let low = self.read(addr) as u16;
        let high = self.read(addr + 1) as u16;
        (high << 8) | low
    }

    fn bne(&mut self) {
        let offset = self.read(self.program_counter) as i8;
        self.program_counter += 1;
        if self.status & ZERO_FLAG == 0 {
            self.program_counter = ((self.program_counter as i16) + offset as i16) as u16;
        }
    }

    fn bmi(&mut self) {
        let offset = self.read(self.program_counter) as i8;
        self.program_counter += 1;
        if self.status & NEGATIVE_FLAG != 0 {
            self.program_counter = ((self.program_counter as i16) + offset as i16) as u16;
        }
    }

    fn zero_page_x(&mut self) -> u16 {
        let base = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        (base + self.x_register as u16) & 0x00FF
    }

    fn zero_page_y(&mut self) -> u16 {
        let base = self.read(self.program_counter) as u16;
        self.program_counter += 1;
        (base + self.y_register as u16) & 0x00FF
    }

    fn pha(&mut self) {
        self.push(self.accumulator);
    }

    fn push(&mut self, value: u8) {
        self.write(0x0100 + self.stack_pointer as u16, value);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1);
    }

    fn pla(&mut self) {
        self.accumulator = self.pull();
        self.update_zero_and_negative_flags(self.accumulator);
    }

    fn pull(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.read(0x0100 + self.stack_pointer as u16)
    }

    // 命令のフェッチ、デコード、実行
    fn execute(&mut self) {
        let opcode = self.read(self.program_counter);
        self.program_counter += 1;

        match opcode {
            0x69 => self.adc_immediate(), // ADC Immediate
            0x25 => self.and_zero_page(), // AND Zero Page
            0x0A => self.asl_accumulator(), // ASL Accumulator
            0x90 => self.bcc_relative(), // BCC Relative
            0xC9 => self.cmp_immediate(), // CMP Immediate
            0xC6 => self.dec_zero_page(), // DEC Zero Page
            0x4D => self.eor_absolute(),  // EOR Absolute
            0xA2 => self.ldx_immediate(), // LDX Immediate
            // ...他の命令...
            _ => unimplemented!("Opcode {:02X} is not implemented", opcode),
        }
    }

    pub fn run(&mut self, memory: &mut Memory) {
        loop {
            self.execute();
            if self.program_counter == 0x1234 {
                break;
            }
            // ここにエミュレーションの実行を停止する条件を追加する
            // 例: 特定のプログラムカウンタ値に到達した場合、または特定の命令を実行した後
        }
    }

    pub fn inspect_memory(&self, memory: &Memory, addr: u16) -> u8 {
        memory.read(addr)
    }


    pub fn inspect_registers(&self) -> Registers {
        Registers {
            accumulator: self.accumulator,
            x_index: self.x_register,
            y_index: self.y_register,
            status: self.status,
            program_counter: self.program_counter,
            stack_pointer: self.stack_pointer,
        }
    }
    // 他の命令も同様にメソッドとして実装...
}


impl CpuState {
    // CPU状態の新しいインスタンスを生成するための関数
    pub fn new() -> Self {
        Self {
            accumulator: 0,
            x_register: 0,
            y_register: 0,
            status: 0,
            program_counter: 0,
            // 他のフィールドの初期化...
        }
    }

}
