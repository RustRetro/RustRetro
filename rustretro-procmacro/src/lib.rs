extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn rustretro_plugin(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let trait_impl: syn::ItemImpl = syn::parse_macro_input!(tokens);

    // Get the type identifier
    let struct_ident = match *trait_impl.self_ty.clone() {
        syn::Type::Path(syn::TypePath { qself: _, path: p }) => {
            let t = p.segments.last().expect("No struct?");
            t.ident.clone()
        }
        _ => panic!("Implement is not on a struct!"),
    };

    let expanded = quote::quote! {
        #trait_impl

        extern crate alloc as _rustretro_plugin_alloc;
        #[no_mangle]
        pub fn __create_core() -> u32 {
            let rom = unsafe {
                let mut buffer = [0u8; 4];
                let length: &[u8] = ::core::slice::from_raw_parts(0 as *const u8, 4);

                buffer.copy_from_slice(length);
                let length = u32::from_le_bytes(buffer);

                ::core::slice::from_raw_parts(4 as *const u8, length as usize)
            };

            let emulator = #struct_ident::create_core(rom);

            ::_rustretro_plugin_alloc::boxed::Box::into_raw(emulator) as u32
        }

        #[no_mangle]
        pub unsafe fn __get_metadata(ptr: u32) {
            let emulator = &mut *(ptr as *mut #struct_ident);

            let metadata = emulator.get_metadata();

            let data = ::rustretro_plugin::serde_json::to_vec(&metadata).unwrap();

            let buffer = ::core::slice::from_raw_parts_mut(0 as *mut u8, data.len() + 4);

            let length = data.len().to_le_bytes();
            buffer[0..4].copy_from_slice(&length);
            buffer[4..].copy_from_slice(&data);
        }

        #[no_mangle]
        pub unsafe fn __controller_input(ptr: u32, input: u32) {
            let emulator = &mut *(ptr as *mut #struct_ident);

            let input = ::rustretro_plugin::ControllerInput::from_bits_truncate(input as u8);
            emulator.controller_input(input);
        }

        #[no_mangle]
        pub unsafe fn __clock_until_frame(ptr: u32) -> u32 {
            let emulator = &mut *(ptr as *mut #struct_ident);
            let frame = emulator.clock_until_frame();

            let buffer = ::core::slice::from_raw_parts_mut(0 as *mut u8, frame.len() + 4);

            let length = frame.len().to_le_bytes();
            buffer[0..4].copy_from_slice(&length);

            ::_rustretro_plugin_alloc::boxed::Box::into_raw(frame.into_boxed_slice()) as *mut u8
                as u32
        }

        #[no_mangle]
        pub unsafe fn __free_frame(ptr: u32, length: u32) {
            ::_rustretro_plugin_alloc::boxed::Box::from_raw(::core::slice::from_raw_parts_mut(
                ptr as *mut u8,
                length as usize,
            ));
        }

        #[no_mangle]
        pub unsafe fn __free_emulator(ptr: u32) {
            ::_rustretro_plugin_alloc::boxed::Box::from_raw(ptr as *mut #struct_ident);
        }
    };

    // Validate the trait is implemented
    match trait_impl.trait_ {
        Some((_, path, _)) => {
            let trait_ident = &path.segments.last().expect("No trait?").ident;
            if trait_ident.to_string() != "RustretroPlugin" {
                panic!("The impl block should implement RustRetroPlugin")
            }
        }
        _ => panic!("Not a trait implement!"),
    }

    TokenStream::from(expanded)
}
