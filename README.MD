# RustRetro
This repo is at the proof of concept stage and not really useable for now.

RustRetro is a multisystem plugin-based emulator. Emulation cores can be compiled in WebAssembly to run within a RustRetro frontend, extending the capabilities to add multiple consoles to it.

## How does that compare to Libretro/RetroArch?
RustRetro uses WebAssembly cores, which differs from LibRetro/RetroArch native cores in multiple ways.  
### Pros:  
#### For End Users
1. Security: A RustRetro core is sandboxed via the WebAssembly VM, which means that a vulnerable or even malicious core cannot do harm to the host system.  
2. Compatibility: Once a core works, it works everywhere. That means that a core that works on a windows computer also works on Mac and Linux, and even phones and inside a web browser.
#### For Core Developpers
1. The cores are "build once, run everywhere". That means that there is no need to automate a huge build system to support all OS/architecture combination.
2. No linking issues: The core and frontend does not need to be linked against the same libc, which can be a pain with some configuration.

### Cons:
1. Because of the WebAssembly JIT, the emulator will most likely run slower then it would on LibRetro. However, because of runtime vectorization, that might now always be the case for end users.
2. For the time being, the cores don't have access to multiple OS features that LibRetro cores have access to. Later on, we might add a permission request system using WASI to allow access to some of these features if the user accepts it. It might also require some refactoring on the core side to allow fully sandboxed emulation. **This also means that multithreading is not supported for now, which can be a major issue for HLE emulators**.
3. The project is in a *very* early stage. Expect a lot of missing features on the frontends and really low availability of cores. The project is not ready to be used as a replacement to libretro in any reasonnable use case, but you're free to play around with it.


## How to compile a core
We will add more detailed instruction after we stabilize a first API, but for now you can refer to `nestadia-rustretro`. For now we have support in the `rustretro-plugin` crate to easily bind a core to the RustRetro API in Rust, but we will try to add a cleaner API for other languages in the future(notably C/C++).

## License
The project is dual licensed Apache/MIT.
