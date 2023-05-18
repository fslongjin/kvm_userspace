use std::path::PathBuf;

use kvm_bindings::{kvm_regs, kvm_sregs, kvm_userspace_memory_region};
use kvm_ioctls::{Kvm, VcpuFd, VmFd};
use libc::{c_void, mmap, MAP_ANONYMOUS, MAP_SHARED, PROT_READ, PROT_WRITE};

extern crate kvm_bindings;
extern crate kvm_ioctls;
extern crate libc;

struct Vm {
    kvm: Kvm,
    vm: VmFd,
    hva_ram_start: usize,
    vcpu: Option<VcpuFd>,
}

impl Vm {
    pub fn new() -> Self {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        Vm {
            kvm,
            vm,
            hva_ram_start: 0,
            vcpu: None,
        }
    }

    fn setup_memory(&mut self, ram_size: usize) {
        println!("setup_memory");
        // 把大小按照4096对齐
        let ram_size = (ram_size + 0xfff) & !0xfff;

        // 使用mmap分配虚拟机的内存
        let ptr = unsafe {
            mmap(
                0 as *mut c_void,
                ram_size,
                PROT_READ | PROT_WRITE,
                MAP_SHARED | MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            panic!("mmap failed");
        }

        self.hva_ram_start = ptr as usize;

        // 设置虚拟机的内存，相当于插入1个内存条
        // 插槽编号为0，物理地址从0开始，大小为ram_size

        let mem_region = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0 as u64,
            memory_size: ram_size as u64,
            userspace_addr: ptr as u64,
            flags: 0,
        };
        unsafe {
            self.vm
                .set_user_memory_region(mem_region)
                .map_err(|e| panic!("set_user_memory_region failed: {:?}", e))
                .unwrap();
        };
    }

    fn setup_cpu(&mut self) {
        // 创建一个虚拟CPU
        let vcpu = self.vm.create_vcpu(0).unwrap();
        self.vcpu = Some(vcpu);
        // 设置虚拟CPU的寄存器

        let mut vcpu_sregs: kvm_sregs = self
            .vcpu
            .as_ref()
            .unwrap()
            .get_sregs()
            .expect("get sregs failed");
        vcpu_sregs.cs.selector = 0;
        vcpu_sregs.cs.base = 0;
        self.vcpu
            .as_ref()
            .unwrap()
            .set_sregs(&vcpu_sregs)
            .expect("set sregs failed");

        let mut vcpu_regs: kvm_regs = self
            .vcpu
            .as_ref()
            .unwrap()
            .get_regs()
            .expect("get regs failed");
        vcpu_regs.rax = 0;
        vcpu_regs.rbx = 0;
        vcpu_regs.rip = 0;
        self.vcpu.as_ref().unwrap().set_regs(&vcpu_regs).unwrap();
    }

    fn load_image(&mut self, image: PathBuf) {
        println!("load_image");
        // 读取kernel.bin文件
        let kernel = std::fs::read(image).unwrap();
        println!("kernel: {:?}", kernel);
        // 把kernel.bin文件写入虚拟机的内存
        let ptr = (self.hva_ram_start) as *mut u8;
        println!(
            "self.hva_ram_start: {:p}, ptr={ptr:?}",
            self.hva_ram_start as *mut u8
        );
        unsafe {
            std::ptr::copy_nonoverlapping(kernel.as_ptr(), ptr, kernel.len());
        }
    }

    fn run(&mut self) {
        println!("run");
        let vcpu = self.vcpu.as_mut().unwrap();
        loop {
            match vcpu.run().expect("run failed") {
                kvm_ioctls::VcpuExit::Hlt => {
                    println!("KVM_EXIT_HLT");
                    // sleep 1s using rust std
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                kvm_ioctls::VcpuExit::IoOut(port, data) => {
                    let data_str = String::from_utf8_lossy(data);
                    print!("{}", data_str);
                }
                kvm_ioctls::VcpuExit::FailEntry(reason, vcpu) => {
                    println!("KVM_EXIT_FAIL_ENTRY");
                    break;
                }
                _ => {
                    println!("Other exit reason");
                    break;
                }
            }
        }
    }
}

fn main() {
    let image = PathBuf::from("./guest_os/kernel.bin");
    let mut vm = Vm::new();

    // 设置虚拟机的内存大小1MB
    let mem_size = 0x1000;
    vm.setup_memory(mem_size);
    vm.setup_cpu();
    vm.load_image(image);
    vm.run();
}
