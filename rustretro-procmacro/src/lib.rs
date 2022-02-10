extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn rustretro_plugin(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let tokens = proc_macro2::TokenStream::from(tokens);
    let item: syn::Item = syn::parse2(tokens).expect("Unable to parse");

    let trait_impl = match item {
        syn::Item::Impl(x) => x,
        _ => panic!("The attribute should be on an impl block"),
    };

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
        pub unsafe fn __rustretro_plugin_create_core(ptr: u32, length: u32) -> u32 {
            let rom = ::core::slice::from_raw_parts(ptr as *const u8, length as usize);
            let emulator = #struct_ident::create_core(rom);

            ::_rustretro_plugin_alloc::boxed::Box::into_raw(emulator) as u32
        }

        #[no_mangle]
        pub unsafe fn __rustretro_plugin_get_metadata(ptr: u32) -> u64 {
            let emulator = &mut *(ptr as *mut #struct_ident);

            let metadata = emulator.get_metadata();

            let data = ::rustretro_plugin::serde_json::to_vec(&metadata).unwrap();
            let length = data.len() as u64;

            let ptr = ::_rustretro_plugin_alloc::boxed::Box::into_raw(data.into_boxed_slice()) as *mut u8 as u64;

            ptr | (length << 32)
        }

        #[no_mangle]
        pub unsafe fn __rustretro_plugin_controller_input(ptr: u32, input: u32) {
            let emulator = &mut *(ptr as *mut #struct_ident);

            let input = ::rustretro_plugin::ControllerInput::from_bits_truncate(input as u8);
            emulator.controller_input(input);
        }

        #[no_mangle]
        pub unsafe fn __rustretro_plugin_clock_until_frame(ptr: u32) -> u64 {
            let emulator = &mut *(ptr as *mut #struct_ident);
            let frame = emulator.clock_until_frame();
            let length = frame.len() as u64;

            let ptr = ::_rustretro_plugin_alloc::boxed::Box::into_raw(frame.into_boxed_slice()) as *mut u8
                as u64;

            ptr | (length << 32)
        }

        #[no_mangle]
        pub unsafe fn __rustretro_plugin_alloc_vec(length: u32) -> u32 {
            Box::into_raw(::_rustretro_plugin_alloc::vec![0u8; length as usize].into_boxed_slice()) as *mut u8 as u32
        }

        #[no_mangle]
        pub unsafe fn __rustretro_plugin_free_vec(ptr: u32, length: u32) {
            ::_rustretro_plugin_alloc::boxed::Box::from_raw(::core::slice::from_raw_parts_mut(
                ptr as *mut u8,
                length as usize,
            ));
        }

        #[no_mangle]
        pub unsafe fn __rustretro_plugin_free_emulator(ptr: u32) {
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
