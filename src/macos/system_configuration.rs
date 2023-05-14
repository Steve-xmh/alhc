use super::cf_network::CFDictionaryRef;

// SCDynamicStoreCopyProxies
#[repr(C)]
pub struct SCDynamicStore(::core::ffi::c_void);

pub type SCDynamicStoreRef = *mut SCDynamicStore;

#[link(name = "SystemConfiguration", kind = "framework")]
extern "C" {
    // CFDictionaryRef SCDynamicStoreCopyProxies(SCDynamicStoreRef store);
    pub fn SCDynamicStoreCopyProxies(store: SCDynamicStoreRef) -> CFDictionaryRef;
}