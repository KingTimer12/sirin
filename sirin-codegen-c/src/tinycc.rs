#![allow(non_camel_case_types, dead_code)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_ulong, c_void};

// ── tipos opacos ─────────────────────────────────────────────────────────────

pub enum TCCState {}

// ── constantes de output type ─────────────────────────────────────────────────

pub const TCC_OUTPUT_MEMORY: c_int = 1;
pub const TCC_OUTPUT_EXE: c_int = 2;
pub const TCC_OUTPUT_DLL: c_int = 4;
pub const TCC_OUTPUT_OBJ: c_int = 3;
pub const TCC_OUTPUT_PREPROCESS: c_int = 5;

// ── tipos de callback ─────────────────────────────────────────────────────────

pub type TCCReallocFunc = unsafe extern "C" fn(ptr: *mut c_void, size: c_ulong) -> *mut c_void;
pub type TCCErrorFunc = unsafe extern "C" fn(opaque: *mut c_void, msg: *const c_char);
pub type TCCBtFunc = unsafe extern "C" fn(
    udata: *mut c_void,
    pc: *mut c_void,
    file: *const c_char,
    line: c_int,
    func: *const c_char,
    msg: *const c_char,
) -> c_int;
pub type SymbolCb = unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char, val: *const c_void);

// ── bindings FFI raw ──────────────────────────────────────────────────────────

#[link(name = "tcc", kind = "static")]
unsafe extern "C" {
    pub fn tcc_set_realloc(my_realloc: Option<TCCReallocFunc>);
    pub fn tcc_new() -> *mut TCCState;
    pub fn tcc_delete(s: *mut TCCState);
    pub fn tcc_set_lib_path(s: *mut TCCState, path: *const c_char);
    pub fn tcc_set_error_func(
        s: *mut TCCState,
        error_opaque: *mut c_void,
        error_func: Option<TCCErrorFunc>,
    );
    pub fn tcc_set_options(s: *mut TCCState, str: *const c_char) -> c_int;
    pub fn tcc_add_include_path(s: *mut TCCState, pathname: *const c_char) -> c_int;
    pub fn tcc_add_sysinclude_path(s: *mut TCCState, pathname: *const c_char) -> c_int;
    pub fn tcc_define_symbol(s: *mut TCCState, sym: *const c_char, value: *const c_char);
    pub fn tcc_undefine_symbol(s: *mut TCCState, sym: *const c_char);
    pub fn tcc_add_file(s: *mut TCCState, filename: *const c_char) -> c_int;
    pub fn tcc_compile_string(s: *mut TCCState, buf: *const c_char) -> c_int;
    pub fn tcc_set_output_type(s: *mut TCCState, output_type: c_int) -> c_int;
    pub fn tcc_add_library_path(s: *mut TCCState, pathname: *const c_char) -> c_int;
    pub fn tcc_add_library(s: *mut TCCState, libraryname: *const c_char) -> c_int;
    pub fn tcc_add_symbol(s: *mut TCCState, name: *const c_char, val: *const c_void) -> c_int;
    pub fn tcc_output_file(s: *mut TCCState, filename: *const c_char) -> c_int;
    pub fn tcc_run(s: *mut TCCState, argc: c_int, argv: *mut *mut c_char) -> c_int;
    pub fn tcc_relocate(s: *mut TCCState) -> c_int;
    pub fn tcc_get_symbol(s: *mut TCCState, name: *const c_char) -> *mut c_void;
    pub fn tcc_list_symbols(s: *mut TCCState, ctx: *mut c_void, symbol_cb: Option<SymbolCb>);
}

// ── wrapper seguro ────────────────────────────────────────────────────────────

pub struct Tcc {
    state: *mut TCCState,
    // guarda erros capturados pelo callback
    errors: Box<Vec<String>>,
}

// callback que captura erros do TCC em vez de printar no stderr
unsafe extern "C" fn error_collector(opaque: *mut c_void, msg: *const c_char) {
    let errors = unsafe { &mut *(opaque as *mut Vec<String>) };
    let msg = unsafe { CStr::from_ptr(msg) }
        .to_string_lossy()
        .into_owned();
    errors.push(msg);
}

impl Tcc {
    pub fn new() -> Result<Self, &'static str> {
        let state = unsafe { tcc_new() };
        if state.is_null() {
            return Err("tcc_new() returned null");
        }

        let mut tcc = Tcc {
            state,
            errors: Box::new(Vec::new()),
        };

        // registra o callback de erro apontando para o Vec interno
        unsafe {
            tcc_set_error_func(
                tcc.state,
                tcc.errors.as_mut() as *mut Vec<String> as *mut c_void,
                Some(error_collector),
            );
        }

        Ok(tcc)
    }

    pub fn set_options(&self, opts: &str) -> Result<(), String> {
        let c = CString::new(opts).unwrap();
        let r = unsafe { tcc_set_options(self.state, c.as_ptr()) };
        if r == -1 {
            Err(self.collect_errors("set_options"))
        } else {
            Ok(())
        }
    }

    /// Define o tipo de saída — deve ser chamado antes de qualquer compilação
    pub fn set_output_type(&self, output_type: c_int) -> Result<(), String> {
        let r = unsafe { tcc_set_output_type(self.state, output_type) };
        if r == -1 {
            Err(self.collect_errors("set_output_type"))
        } else {
            Ok(())
        }
    }

    /// Compila uma string de código C
    pub fn compile_string(&self, src: &str) -> Result<(), String> {
        let c = CString::new(src).unwrap();
        let r = unsafe { tcc_compile_string(self.state, c.as_ptr()) };
        if r == -1 {
            Err(self.collect_errors("compile_string"))
        } else {
            Ok(())
        }
    }

    /// Adiciona um arquivo C, objeto ou biblioteca
    pub fn add_file(&self, path: &str) -> Result<(), String> {
        let c = CString::new(path).unwrap();
        let r = unsafe { tcc_add_file(self.state, c.as_ptr()) };
        if r == -1 {
            Err(self.collect_errors("add_file"))
        } else {
            Ok(())
        }
    }

    /// Gera um executável em disco
    pub fn output_file(&self, path: &str) -> Result<(), String> {
        let c = CString::new(path).unwrap();
        let r = unsafe { tcc_output_file(self.state, c.as_ptr()) };
        if r == -1 {
            Err(self.collect_errors("output_file"))
        } else {
            Ok(())
        }
    }

    /// Adiciona um include path
    pub fn add_include_path(&self, path: &str) -> Result<(), String> {
        let c = CString::new(path).unwrap();
        let r = unsafe { tcc_add_include_path(self.state, c.as_ptr()) };
        if r == -1 {
            Err(self.collect_errors("add_include_path"))
        } else {
            Ok(())
        }
    }

    /// Define onde estão os headers e runtime do TCC
    pub fn set_lib_path(&self, path: &str) {
        let c = CString::new(path).unwrap();
        unsafe { tcc_set_lib_path(self.state, c.as_ptr()) };
    }

    /// Adiciona um diretório de busca para bibliotecas (.a / .def)
    pub fn add_library_path(&self, path: &str) -> Result<(), String> {
        let c = CString::new(path).unwrap();
        let r = unsafe { tcc_add_library_path(self.state, c.as_ptr()) };
        if r == -1 {
            Err(self.collect_errors("add_library_path"))
        } else {
            Ok(())
        }
    }

    fn collect_errors(&self, ctx: &str) -> String {
        if self.errors.is_empty() {
            format!("tcc error in {ctx} (no message)")
        } else {
            self.errors.join("\n")
        }
    }
}

impl Drop for Tcc {
    fn drop(&mut self) {
        unsafe { tcc_delete(self.state) };
    }
}
