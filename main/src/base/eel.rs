use crate::base::allocator::GLOBAL_ALLOCATOR;
use crate::base::bindings::root;
use crate::base::bindings::root::eel_function_table;
use reaper_medium::ReaperStr;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::os::raw::c_void;
use std::ptr::null_mut;

#[derive(Debug)]
pub struct Vm {
    vm_ctx: root::NSEEL_VMCTX,
    function_table: Box<root::eel_function_table>,
}

unsafe impl Send for Vm {}

#[derive(Debug)]
pub struct Program(root::NSEEL_CODEHANDLE);

unsafe impl Send for Program {}

#[derive(Copy, Clone, Debug)]
pub struct Variable(*mut f64);

unsafe impl Send for Variable {}

// TODO-medium It's actually not Sync. It's safe in our case because we know that we never use an
//  EEL program at the same time in 2 threads.
unsafe impl Sync for Vm {}
unsafe impl Sync for Program {}
unsafe impl Sync for Variable {}

impl Vm {
    pub fn new() -> Vm {
        let vm_ctx = unsafe { root::NSEEL_VM_alloc() };
        let mut function_table = Box::new(eel_function_table {
            list: null_mut(),
            list_size: 0,
        });
        unsafe {
            root::NSEEL_VM_SetFunctionTable(vm_ctx, &mut *function_table as *mut _);
        }
        Vm {
            vm_ctx,
            function_table,
        }
    }

    pub fn register_single_arg_function(
        &mut self,
        name: &'static ReaperStr,
        f: unsafe extern "C" fn(opaque: *mut c_void, amt: *mut f64) -> f64,
    ) {
        unsafe {
            root::NSEEL_addfunc_ret_type(
                name.as_c_str().as_ptr(),
                1,
                1,
                Some(root::NSEEL_PProc_THIS),
                f as *mut c_void,
                &mut *self.function_table as *mut _,
            );
        }
    }

    pub fn register_variable(&self, name: &str) -> Variable {
        let c_string = CString::new(name).expect("variable name is not valid UTF-8");
        let ptr = unsafe { root::NSEEL_VM_regvar(self.vm_ctx, c_string.as_ptr()) };
        Variable(ptr)
    }

    pub fn register_and_set_variable(&self, name: &str, value: f64) -> Variable {
        let v = self.register_variable(name);
        unsafe {
            v.set(value);
        }
        v
    }

    pub fn get_mem_slice(&self, index: u32, size: u32) -> &[f64] {
        let mut valid_count = MaybeUninit::zeroed();
        let ptr = unsafe {
            root::NSEEL_VM_getramptr_noalloc(self.vm_ctx, index, valid_count.as_mut_ptr())
        };
        let valid_count = unsafe { valid_count.assume_init() };
        if ptr.is_null() || valid_count <= 0 {
            return &[];
        }
        let slice_len = std::cmp::min(valid_count as u32, size);
        let slice = std::ptr::slice_from_raw_parts(ptr, slice_len as _);
        unsafe { &*slice }
    }

    pub fn compile(&self, code: &str) -> Result<Program, String> {
        if code.trim().is_empty() {
            return Err("Empty".to_owned());
        }
        let c_string = CString::new(code).map_err(|_| "Code is not valid UTF-8")?;
        let code_handle = unsafe { root::NSEEL_code_compile(self.vm_ctx, c_string.as_ptr(), 0) };
        if code_handle.is_null() {
            let error = unsafe { root::NSEEL_code_getcodeerror(self.vm_ctx) };
            if error.is_null() {
                return Err("Unknown error".to_string());
            }
            let c_str = unsafe { CStr::from_ptr(error) };
            let string = c_str
                .to_owned()
                .into_string()
                .unwrap_or_else(|_| "Couldn't convert error to string".to_string());
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
        GLOBAL_ALLOCATOR.dealloc_foreign_value(root::NSEEL_VM_free, self.vm_ctx);
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        GLOBAL_ALLOCATOR.dealloc_foreign_value(root::NSEEL_code_free, self.0);
    }
}

#[no_mangle]
extern "C" fn NSEEL_HOSTSTUB_EnterMutex() {}

#[no_mangle]
extern "C" fn NSEEL_HOSTSTUB_LeaveMutex() {}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

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
        assert_abs_diff_eq!(y_result, 43.0);
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
        assert_abs_diff_eq!(slice[0], 43.0);
        assert_abs_diff_eq!(slice[1], 44.0);
        assert_abs_diff_eq!(slice[2], 47.0);
    }
}
