fn main() {
    #[cfg(feature = "python")]
    {
        // Print cfg flags that pyo3 needs
        pyo3_build_config::use_pyo3_cfgs();

        // Get interpreter config and emit link args for embedding Python
        let config = pyo3_build_config::get();

        // Print library search path
        if let Some(lib_dir) = &config.lib_dir {
            println!("cargo:rustc-link-search=native={}", lib_dir);
        }

        // Print library name - for embedding we need to link against Python
        if let Some(lib_name) = &config.lib_name {
            println!("cargo:rustc-link-lib={}", lib_name);
        }
    }
}
