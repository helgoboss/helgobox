use crate::core::bindings::root;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;

#[derive(Debug)]
pub struct Vm(root::NSEEL_VMCTX);

#[derive(Debug)]
pub struct Program(root::NSEEL_CODEHANDLE);

#[derive(Copy, Clone, Debug)]
pub struct Variable(*mut f64);

impl Vm {
    pub fn new() -> Vm {
        Vm(unsafe { root::NSEEL_VM_alloc() })
    }

    pub fn register_variable(&self, name: &str) -> Variable {
        let c_string = CString::new(name).expect("variable name is not valid UTF-8");
        let ptr = unsafe { root::NSEEL_VM_regvar(self.0, c_string.as_ptr()) };
        Variable(ptr)
    }

    pub fn get_mem_slice(&self, index: u32, size: u32) -> &[f64] {
        let mut valid_count = MaybeUninit::zeroed();
        let ptr =
            unsafe { root::NSEEL_VM_getramptr_noalloc(self.0, index, valid_count.as_mut_ptr()) };
        let valid_count = unsafe { valid_count.assume_init() };
        if ptr.is_null() || valid_count <= 0 {
            return &[];
        }
        let slice_len = std::cmp::min(valid_count as u32, size);
        let slice = std::ptr::slice_from_raw_parts(ptr, slice_len as _);
        unsafe { &*slice }
    }

    pub fn compile(&self, code: &str) -> Result<Program, String> {
        let c_string = CString::new(code).map_err(|_| "code is not valid UTF-8")?;
        let code_handle = unsafe { root::NSEEL_code_compile(self.0, c_string.as_ptr(), 0) };
        if code_handle.is_null() {
            let error = unsafe { root::NSEEL_code_getcodeerror(self.0) };
            if error.is_null() {
                return Err("unknown error".to_string());
            }
            let c_str = unsafe { CStr::from_ptr(error) };
            let string = c_str
                .to_owned()
                .into_string()
                .unwrap_or_else(|_| "couldn't convert error to string".to_string());
            return Err(string);
        }
        Ok(Program(code_handle))
    }
}

impl Program {
    pub unsafe fn execute(&self) {
        root::NSEEL_code_execute(self.0);
    }
}

impl Variable {
    pub unsafe fn get(&self) -> f64 {
        *self.0
    }

    pub unsafe fn set(&self, value: f64) {
        *self.0 = value;
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        unsafe {
            root::NSEEL_VM_free(self.0);
        }
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        unsafe {
            root::NSEEL_code_free(self.0);
        }
    }
}

#[no_mangle]
extern "C" fn NSEEL_HOSTSTUB_EnterMutex() {}

#[no_mangle]
extern "C" fn NSEEL_HOSTSTUB_LeaveMutex() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        // Given
        let vm = Vm::new();
        let x = vm.register_variable("x");
        let y = vm.register_variable("y");
        let program = vm.compile("y = x + 1;").expect("couldn't compile");
        // // When
        let y_result = unsafe {
            x.set(42.0);
            y.set(0.0);
            program.execute();
            y.get()
        };
        // Then
        assert_eq!(y_result, 43.0);
    }

    #[test]
    fn get_mem_slice() {
        // Given
        let vm = Vm::new();
        let x = vm.register_variable("x");
        let program = vm
            .compile("y[0] = x + 1; y[1] = x + 2; y[2] = x + 5")
            .expect("couldn't compile");
        // // When
        let slice = unsafe {
            x.set(42.0);
            program.execute();
            vm.get_mem_slice(0, 3)
        };
        // Then
        assert_eq!(slice[0], 43.0);
        assert_eq!(slice[1], 44.0);
        assert_eq!(slice[2], 47.0);
    }
}
