#include <windows.h>
#include <dxgi1_2.h>
#include <cstdio>
int main() {
    IDXGIFactory1* factory = nullptr;
    if (FAILED(CreateDXGIFactory1(__uuidof(IDXGIFactory1), (void**)&factory))) return 1;
    IDXGIAdapter1* adapter = nullptr;
    for (UINT index = 0; factory->EnumAdapters1(index, &adapter) == S_OK; ++index) {
        DXGI_ADAPTER_DESC1 desc{};
        if (SUCCEEDED(adapter->GetDesc1(&desc)) && !(desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE))
            std::printf("%08x:%08x:luid:%08x%08x\n", desc.VendorId, desc.DeviceId,
                static_cast<unsigned int>(desc.AdapterLuid.HighPart), desc.AdapterLuid.LowPart);
        adapter->Release();
    }
    factory->Release();
}
