//! Minimal wpcap.dll binding, loaded at runtime. End users have the Npcap runtime but not the SDK
//! import library, so linking against it at build time is not an option.
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::net::Ipv4Addr;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use libloading::Library;
use libloading::os::windows::{Library as WinLibrary, LOAD_LIBRARY_SEARCH_SYSTEM32, LOAD_WITH_ALTERED_SEARCH_PATH};

const ERRBUF: usize = 256;
const AF_INET: u16 = 2;

#[repr(C)]
#[allow(dead_code)] // layout mirror of the C struct
struct PcapIf {
    next: *mut PcapIf,
    name: *const c_char,
    description: *const c_char,
    addresses: *mut PcapAddr,
    flags: c_uint,
}

#[repr(C)]
#[allow(dead_code)] // layout mirror of the C struct
struct PcapAddr {
    next: *mut PcapAddr,
    addr: *const SockAddr,
    netmask: *const c_void,
    broadaddr: *const c_void,
    dstaddr: *const c_void,
}

/// `sockaddr`: family, then for AF_INET `data` = port (2 bytes) + IPv4 address (4 bytes).
#[repr(C)]
#[allow(dead_code)] // layout mirror of the C struct
struct SockAddr {
    family: u16,
    data: [u8; 14],
}

/// `pcap_pkthdr` with Windows' 32-bit `long` timeval.
#[repr(C)]
#[allow(dead_code)] // layout mirror of the C struct
struct PktHdr {
    ts_sec: i32,
    ts_usec: i32,
    caplen: u32,
    len: u32,
}

#[repr(C)]
#[allow(dead_code)] // layout mirror of the C struct
struct BpfProgram {
    len: c_uint,
    insns: *mut c_void,
}

type FindAllDevs = unsafe extern "C" fn(*mut *mut PcapIf, *mut c_char) -> c_int;
type FreeAllDevs = unsafe extern "C" fn(*mut PcapIf);
type OpenLive = unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, *mut c_char) -> *mut c_void;
type Compile = unsafe extern "C" fn(*mut c_void, *mut BpfProgram, *const c_char, c_int, c_uint) -> c_int;
type SetFilter = unsafe extern "C" fn(*mut c_void, *mut BpfProgram) -> c_int;
type FreeCode = unsafe extern "C" fn(*mut BpfProgram);
type NextEx = unsafe extern "C" fn(*mut c_void, *mut *mut PktHdr, *mut *const u8) -> c_int;
type Datalink = unsafe extern "C" fn(*mut c_void) -> c_int;
type Close = unsafe extern "C" fn(*mut c_void);

pub struct Device {
    pub name: String,
    pub description: String,
    pub ipv4: Vec<Ipv4Addr>,
}

pub struct Npcap {
    _lib: Library,
    find_all: FindAllDevs,
    free_all: FreeAllDevs,
    open_live: OpenLive,
    compile: Compile,
    set_filter: SetFilter,
    free_code: FreeCode,
    next_ex: NextEx,
    datalink: Datalink,
    close: Close,
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// `Npcap::load` could not load wpcap.dll; `app::classify` finds it with `downcast_ref`.
#[derive(Debug)]
pub struct NotInstalled(pub PathBuf);

impl std::fmt::Display for NotInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot load {} - is Npcap installed?", self.0.display())
    }
}

/// Whether this process is elevated (`shell32!IsUserAnAdmin`); `false` when it cannot be asked.
pub fn is_elevated() -> bool {
    type IsUserAnAdmin = unsafe extern "system" fn() -> i32;
    // System32 only: never pick up a planted shell32.dll next to the exe
    let Ok(lib) = (unsafe { WinLibrary::load_with_flags("shell32.dll", LOAD_LIBRARY_SEARCH_SYSTEM32) }) else {
        return false;
    };
    match unsafe { lib.get::<IsUserAnAdmin>(b"IsUserAnAdmin\0") } {
        Ok(f) => unsafe { f() != 0 },
        Err(_) => false,
    }
}

/// The Windows System32 folder (`kernel32!GetSystemDirectoryW`); an environment variable could point anywhere.
fn system_dir() -> PathBuf {
    type GetSystemDirectoryW = unsafe extern "system" fn(*mut u16, u32) -> u32;
    let fallback = || PathBuf::from("C:\\Windows\\System32");
    let Ok(lib) = (unsafe { WinLibrary::load_with_flags("kernel32.dll", LOAD_LIBRARY_SEARCH_SYSTEM32) }) else {
        return fallback();
    };
    let Ok(get) = (unsafe { lib.get::<GetSystemDirectoryW>(b"GetSystemDirectoryW\0") }) else {
        return fallback();
    };
    let mut buf = [0u16; 260];
    let len = unsafe { get(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    // 0 = failed; more than the buffer = it was too small
    if len == 0 || len >= buf.len() {
        return fallback();
    }
    PathBuf::from(String::from_utf16_lossy(&buf[..len]))
}

impl Npcap {
    pub fn load() -> Result<Self> {
        let path = system_dir().join("Npcap").join("wpcap.dll");
        // altered search path: wpcap.dll's own directory is searched for Packet.dll
        let lib: Library = unsafe { WinLibrary::load_with_flags(&path, LOAD_WITH_ALTERED_SEARCH_PATH) }
            .with_context(|| NotInstalled(path.clone()))?
            .into();
        unsafe {
            Ok(Self {
                find_all: *lib.get::<FindAllDevs>(b"pcap_findalldevs\0")?,
                free_all: *lib.get::<FreeAllDevs>(b"pcap_freealldevs\0")?,
                open_live: *lib.get::<OpenLive>(b"pcap_open_live\0")?,
                compile: *lib.get::<Compile>(b"pcap_compile\0")?,
                set_filter: *lib.get::<SetFilter>(b"pcap_setfilter\0")?,
                free_code: *lib.get::<FreeCode>(b"pcap_freecode\0")?,
                next_ex: *lib.get::<NextEx>(b"pcap_next_ex\0")?,
                datalink: *lib.get::<Datalink>(b"pcap_datalink\0")?,
                close: *lib.get::<Close>(b"pcap_close\0")?,
                _lib: lib,
            })
        }
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let mut errbuf = [0 as c_char; ERRBUF];
        let mut head: *mut PcapIf = std::ptr::null_mut();
        if unsafe { (self.find_all)(&mut head, errbuf.as_mut_ptr()) } != 0 {
            bail!("pcap_findalldevs: {}", cstr(errbuf.as_ptr()));
        }
        let mut out = Vec::new();
        let mut cur = head;
        while !cur.is_null() {
            let d = unsafe { &*cur };
            let mut ipv4 = Vec::new();
            let mut a = d.addresses;
            while !a.is_null() {
                let addr = unsafe { &*a };
                if !addr.addr.is_null() {
                    let sa = unsafe { &*addr.addr };
                    if sa.family == AF_INET {
                        ipv4.push(Ipv4Addr::new(sa.data[2], sa.data[3], sa.data[4], sa.data[5]));
                    }
                }
                a = addr.next;
            }
            out.push(Device { name: cstr(d.name), description: cstr(d.description), ipv4 });
            cur = d.next;
        }
        unsafe { (self.free_all)(head) };
        Ok(out)
    }

    /// Open `device` for live capture (no promiscuous mode, 100 ms read timeout) with a BPF filter.
    pub fn open(&self, device: &str, filter: &str) -> Result<Capture<'_>> {
        let name = CString::new(device)?;
        let mut errbuf = [0 as c_char; ERRBUF];
        let handle = unsafe { (self.open_live)(name.as_ptr(), 262144, 0, 100, errbuf.as_mut_ptr()) };
        if handle.is_null() {
            bail!("pcap_open_live({device}): {} (run as Administrator?)", cstr(errbuf.as_ptr()));
        }
        let cap = Capture { api: self, handle }; // Drop closes the handle on any error below
        let f = CString::new(filter)?;
        let mut prog = BpfProgram { len: 0, insns: std::ptr::null_mut() };
        if unsafe { (self.compile)(handle, &mut prog, f.as_ptr(), 1, 0xffff_ffff) } != 0 {
            bail!("invalid capture filter {filter:?}");
        }
        let rc = unsafe { (self.set_filter)(handle, &mut prog) };
        unsafe { (self.free_code)(&mut prog) };
        if rc != 0 {
            bail!("pcap_setfilter failed");
        }
        Ok(cap)
    }
}

pub struct Capture<'a> {
    api: &'a Npcap,
    handle: *mut c_void,
}

impl Capture<'_> {
    pub fn linktype(&self) -> i32 {
        unsafe { (self.api.datalink)(self.handle) }
    }

    /// Next frame as `(timestamp in µs since Unix epoch, bytes)`; `None` on read timeout.
    pub fn next_packet(&mut self) -> Result<Option<(u64, Vec<u8>)>> {
        let mut hdr: *mut PktHdr = std::ptr::null_mut();
        let mut data: *const u8 = std::ptr::null();
        match unsafe { (self.api.next_ex)(self.handle, &mut hdr, &mut data) } {
            1 => {
                let h = unsafe { &*hdr };
                let bytes = unsafe { std::slice::from_raw_parts(data, h.caplen as usize) }.to_vec();
                let ts = h.ts_sec as u32 as u64 * 1_000_000 + h.ts_usec as u32 as u64;
                Ok(Some((ts, bytes)))
            }
            0 => Ok(None),
            rc => bail!("pcap_next_ex returned {rc}"),
        }
    }
}

impl Drop for Capture<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.close)(self.handle) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_dir_is_the_real_system32() {
        let dir = system_dir();
        assert!(dir.ends_with("System32") || dir.ends_with("system32"), "{dir:?}");
        assert!(dir.join("kernel32.dll").exists());
    }

    #[test]
    fn is_elevated_answers_without_panicking() {
        let _ = is_elevated();
    }

    #[test]
    #[ignore = "needs the Npcap runtime installed"]
    fn lists_at_least_one_device() {
        let n = Npcap::load().unwrap();
        let devs = n.devices().unwrap();
        assert!(!devs.is_empty());
        assert!(devs.iter().all(|d| d.name.starts_with("\\Device\\NPF_")));
    }

    #[test]
    #[ignore = "needs the Npcap runtime installed"]
    fn rejects_a_bad_filter() {
        let n = Npcap::load().unwrap();
        let dev = &n.devices().unwrap()[0];
        assert!(n.open(&dev.name, "this is not a bpf filter").is_err());
    }
}
