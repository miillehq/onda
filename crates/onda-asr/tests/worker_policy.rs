use onda_asr::{ExecutionBackend, ExecutionPreference};

#[test]
fn a_cpu_request_disables_gpu_in_both_generic_and_metal_workers() {
    for metal_build in [false, true] {
        assert_eq!(
            ExecutionBackend::bundled(ExecutionPreference::CpuOnly, metal_build),
            ExecutionBackend::Cpu
        );
        assert!(
            !ExecutionBackend::Cpu.uses_gpu(metal_build),
            "a CPU request enabled a GPU in the Metal build"
        );
        assert!(ExecutionBackend::Vulkan.uses_gpu(metal_build));
    }
    for preference in [ExecutionPreference::Auto, ExecutionPreference::GpuPreferred] {
        assert_eq!(
            ExecutionBackend::bundled(preference, false),
            ExecutionBackend::Cpu
        );
        assert_eq!(
            ExecutionBackend::bundled(preference, true),
            ExecutionBackend::Metal
        );
    }
    assert!(ExecutionBackend::Metal.uses_gpu(true));
    assert!(!ExecutionBackend::Metal.uses_gpu(false));
}
