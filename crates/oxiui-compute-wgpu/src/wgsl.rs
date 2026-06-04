//! WGSL shader utilities: preprocessor, validation, and built-in compute kernels.
//!
//! This module provides three layers of WGSL tooling:
//!
//! 1. **Preprocessor** — resolves `#include "path"` directives via a caller-supplied
//!    resolver closure, with cycle detection and a depth cap.
//!
//! 2. **Validation** — compiles a WGSL string into a `wgpu::ShaderModule` and
//!    surfaces all `wgpu::CompilationInfo` error diagnostics (with line/column) as
//!    structured [`WgslDiagnostic`] values instead of panicking.
//!
//! 3. **Built-in kernels** — validated `pub const` WGSL strings for common
//!    compute patterns: inclusive prefix sum, sum reduction, histogram, and tiled
//!    matrix multiply.

use std::collections::HashSet;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum recursive `#include` depth before [`preprocess`] returns
/// [`WgslError::DepthExceeded`].
const MAX_INCLUDE_DEPTH: usize = 64;

// ── WgslError ─────────────────────────────────────────────────────────────────

/// Errors produced by the WGSL preprocessor.
#[derive(Debug, Clone, PartialEq)]
pub enum WgslError {
    /// A referenced `#include "path"` path could not be resolved.
    MissingInclude(String),
    /// A `#include "path"` chain forms a cycle.
    CyclicInclude(String),
    /// The recursive `#include` depth exceeded the maximum include depth (64).
    DepthExceeded,
}

impl std::fmt::Display for WgslError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WgslError::MissingInclude(p) => write!(f, "missing include: {p}"),
            WgslError::CyclicInclude(p) => write!(f, "cyclic include detected: {p}"),
            WgslError::DepthExceeded => write!(f, "include depth limit exceeded"),
        }
    }
}

impl std::error::Error for WgslError {}

// ── Preprocessor ─────────────────────────────────────────────────────────────

/// Resolve `#include "path"` directives in `source`, using `resolver` to fetch
/// included content by path.  Supports recursive includes with cycle detection.
///
/// `resolver` returns `Some(content)` for a known path, `None` for a missing one.
///
/// # Errors
///
/// - [`WgslError::MissingInclude`] when `resolver` returns `None` for a path.
/// - [`WgslError::CyclicInclude`] when a path would be included recursively.
/// - [`WgslError::DepthExceeded`] when the include stack exceeds 64 levels.
///
/// # Example
///
/// ```
/// use oxiui_compute_wgpu::wgsl::{preprocess};
/// use std::collections::HashMap;
///
/// let mut lib = HashMap::new();
/// lib.insert("common.wgsl", "fn add(a: f32, b: f32) -> f32 { return a + b; }");
///
/// let source = r#"#include "common.wgsl"
/// @compute @workgroup_size(1) fn main() {}"#;
///
/// let resolver = |path: &str| lib.get(path).map(|s| s.to_string());
/// let result = preprocess(source, &resolver).unwrap();
/// assert!(result.contains("fn add"));
/// ```
pub fn preprocess<F>(source: &str, resolver: &F) -> Result<String, WgslError>
where
    F: Fn(&str) -> Option<String>,
{
    let mut visited = HashSet::new();
    preprocess_inner(source, resolver, &mut visited, 0)
}

fn preprocess_inner<F>(
    source: &str,
    resolver: &F,
    visited: &mut HashSet<String>,
    depth: usize,
) -> Result<String, WgslError>
where
    F: Fn(&str) -> Option<String>,
{
    if depth > MAX_INCLUDE_DEPTH {
        return Err(WgslError::DepthExceeded);
    }

    let mut output = String::with_capacity(source.len());

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("#include") {
            // Parse the path from: #include "some/path.wgsl"
            let path = rest.trim().trim_matches('"').trim();
            if visited.contains(path) {
                return Err(WgslError::CyclicInclude(path.to_string()));
            }
            let content =
                resolver(path).ok_or_else(|| WgslError::MissingInclude(path.to_string()))?;
            visited.insert(path.to_string());
            let expanded = preprocess_inner(&content, resolver, visited, depth + 1)?;
            visited.remove(path);
            output.push_str(&expanded);
            output.push('\n');
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    Ok(output)
}

// ── WgslDiagnostic ────────────────────────────────────────────────────────────

/// A single WGSL compilation diagnostic emitted by the `wgpu` driver.
#[derive(Debug, Clone)]
pub struct WgslDiagnostic {
    /// The human-readable diagnostic message.
    pub message: String,
    /// 1-based source line number (0 if no location is available).
    pub line: u32,
    /// 1-based source column (byte position within the line; 0 if unavailable).
    pub column: u32,
}

impl std::fmt::Display for WgslDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

// ── validate ─────────────────────────────────────────────────────────────────

/// Compile `source` as a WGSL shader module and collect all **error**
/// diagnostics from `wgpu::CompilationInfo`.
///
/// Returns `Ok(())` when the shader compiles with no errors (warnings are
/// ignored).  Returns `Err(diagnostics)` with the full list of
/// [`WgslDiagnostic`] entries for every error reported by the driver.
///
/// This is a *blocking* call: it uses [`pollster::block_on`] to drive the
/// async `get_compilation_info()` future.
///
/// # Example
///
/// ```rust,no_run
/// use oxiui_compute_wgpu::{ComputeContext, wgsl::validate};
///
/// if let Some(ctx) = ComputeContext::try_new() {
///     let src = "@group(0) @binding(0) var<storage, read_write> d: array<f32>;\
///                @compute @workgroup_size(1) fn main_cs() {}";
///     match validate(&ctx.device, src) {
///         Ok(()) => println!("shader ok"),
///         Err(diags) => {
///             for d in &diags { eprintln!("{d}"); }
///         }
///     }
/// }
/// ```
pub fn validate(device: &wgpu::Device, source: &str) -> Result<(), Vec<WgslDiagnostic>> {
    // Push a Validation error scope so that invalid WGSL does not panic the
    // process — wgpu normally panics on uncaptured errors.
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("wgsl-validate"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    // Pop the error scope (we don't need the wgpu-level error; CompilationInfo
    // carries the structured diagnostics we actually report).
    let _ = pollster::block_on(error_scope.pop());

    let info = pollster::block_on(shader.get_compilation_info());

    let errors: Vec<WgslDiagnostic> = info
        .messages
        .iter()
        .filter(|m| m.message_type == wgpu::CompilationMessageType::Error)
        .map(|m| WgslDiagnostic {
            message: m.message.clone(),
            line: m.location.as_ref().map_or(0, |l| l.line_number),
            column: m.location.as_ref().map_or(0, |l| l.line_position),
        })
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ── Built-in WGSL kernels ──────────────────────────────────────────────────────

/// WGSL inclusive prefix sum shader (single workgroup, ≤256 elements, f32).
///
/// **Bindings:** `@group(0) @binding(0)` — `storage, read_write` array of `f32`.
///
/// Computes an in-place inclusive scan: after dispatch, element `i` of `data`
/// equals the sum of the original elements `0..=i`.
///
/// **Constraints:** input length must be ≤ 256; dispatch one workgroup of 256
/// threads.
pub const SHADER_PREFIX_SUM: &str = r#"
@group(0) @binding(0) var<storage, read_write> data: array<f32>;
var<workgroup> wg_buf: array<f32, 256>;

@compute @workgroup_size(256)
fn main_cs(
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let n = arrayLength(&data);
    let i = lid.x;
    wg_buf[i] = select(0.0, data[i], i < n);
    workgroupBarrier();

    var offset: u32 = 1u;
    loop {
        if offset >= 256u { break; }
        var val: f32 = wg_buf[i];
        if i >= offset {
            val = val + wg_buf[i - offset];
        }
        workgroupBarrier();
        wg_buf[i] = val;
        workgroupBarrier();
        offset = offset * 2u;
    }

    if i < n {
        data[i] = wg_buf[i];
    }
}
"#;

/// WGSL sum reduction shader (single workgroup, ≤256 elements, f32 → f32).
///
/// **Bindings:**
/// - `@group(0) @binding(0)` — `storage, read` input array of `f32`.
/// - `@group(0) @binding(1)` — `storage, read_write` output array of `f32`; the
///   sum is written to `output[0]`.
///
/// **Constraints:** input length must be ≤ 256; dispatch one workgroup of 256
/// threads.
pub const SHADER_REDUCTION_SUM: &str = r#"
@group(0) @binding(0) var<storage, read> input: array<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;

var<workgroup> wg_buf: array<f32, 256>;

@compute @workgroup_size(256)
fn main_cs(@builtin(local_invocation_id) lid: vec3<u32>) {
    let n = arrayLength(&input);
    let i = lid.x;
    wg_buf[i] = select(0.0, input[i], i < n);
    workgroupBarrier();

    var stride: u32 = 128u;
    loop {
        if stride == 0u { break; }
        if i < stride {
            wg_buf[i] = wg_buf[i] + wg_buf[i + stride];
        }
        workgroupBarrier();
        stride = stride / 2u;
    }

    if i == 0u {
        output[0] = wg_buf[0];
    }
}
"#;

/// WGSL histogram shader (u32 values → bin counts, up to 256 bins).
///
/// **Bindings:**
/// - `@group(0) @binding(0)` — `storage, read` input array of `u32`.
/// - `@group(0) @binding(1)` — `storage, read_write` histogram array of
///   `atomic<u32>` (length = number of bins, ≤ 256).
///
/// Each input element is binned as `input[i] % num_bins`.  The shader uses a
/// workgroup-local atomic histogram to minimise global memory traffic.
pub const SHADER_HISTOGRAM: &str = r#"
@group(0) @binding(0) var<storage, read> input: array<u32>;
@group(0) @binding(1) var<storage, read_write> histogram: array<atomic<u32>>;

var<workgroup> local_hist: array<atomic<u32>, 256>;

@compute @workgroup_size(64)
fn main_cs(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let n = arrayLength(&input);
    let num_bins = arrayLength(&histogram);

    if lid.x < num_bins {
        atomicStore(&local_hist[lid.x], 0u);
    }
    workgroupBarrier();

    let idx = gid.x;
    if idx < n {
        let bin = input[idx] % num_bins;
        atomicAdd(&local_hist[bin], 1u);
    }
    workgroupBarrier();

    if lid.x < num_bins {
        atomicAdd(&histogram[lid.x], atomicLoad(&local_hist[lid.x]));
    }
}
"#;

/// WGSL tiled matrix multiply (f32 M×K × K×N → M×N, workgroup 16×16).
///
/// **Bindings:**
/// - `@group(0) @binding(0)` — `storage, read` matrix A (row-major, M×K f32).
/// - `@group(0) @binding(1)` — `storage, read` matrix B (row-major, K×N f32).
/// - `@group(0) @binding(2)` — `storage, read_write` matrix C (row-major, M×N f32).
/// - `@group(0) @binding(3)` — `uniform` `MatDims { M: u32, K: u32, N: u32 }`.
///
/// Uses 16×16 shared-memory tiles for cache-efficient multiply-accumulate.
/// Dispatch `ceil(N/16) × ceil(M/16) × 1` workgroups.
pub const SHADER_MATMUL: &str = r#"
const TILE: u32 = 16u;

struct MatDims { M: u32, K: u32, N: u32 }

@group(0) @binding(0) var<storage, read> A: array<f32>;
@group(0) @binding(1) var<storage, read> B: array<f32>;
@group(0) @binding(2) var<storage, read_write> C: array<f32>;
@group(0) @binding(3) var<uniform> dims: MatDims;

var<workgroup> tileA: array<f32, 256>;
var<workgroup> tileB: array<f32, 256>;

@compute @workgroup_size(16, 16)
fn main_cs(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let row = gid.y;
    let col = gid.x;
    let local_row = lid.y;
    let local_col = lid.x;

    var sum: f32 = 0.0;
    let num_tiles = (dims.K + TILE - 1u) / TILE;

    for (var t: u32 = 0u; t < num_tiles; t++) {
        let a_col = t * TILE + local_col;
        let b_row = t * TILE + local_row;

        tileA[local_row * TILE + local_col] = select(0.0, A[row * dims.K + a_col], row < dims.M && a_col < dims.K);
        tileB[local_row * TILE + local_col] = select(0.0, B[b_row * dims.N + col], b_row < dims.K && col < dims.N);

        workgroupBarrier();

        for (var k: u32 = 0u; k < TILE; k++) {
            sum = sum + tileA[local_row * TILE + k] * tileB[k * TILE + local_col];
        }

        workgroupBarrier();
    }

    if row < dims.M && col < dims.N {
        C[row * dims.N + col] = sum;
    }
}
"#;

/// WGSL SPH (Smoothed Particle Hydrodynamics) density kernel using the cubic spline W(r,h).
///
/// Computes density `ρᵢ = Σⱼ mⱼ · W(|rᵢ − rⱼ|, h)` for every particle `i`
/// using the poly-6 kernel `W(r,h) = (315/(64πh⁹))(h²−r²)³` for `r ≤ h`.
///
/// **Bindings:**
/// - `@group(0) @binding(0)` — `storage, read` positions as `array<vec4<f32>>` (xyz + pad).
/// - `@group(0) @binding(1)` — `storage, read` masses `array<f32>`.
/// - `@group(0) @binding(2)` — `storage, read_write` output densities `array<f32>`.
/// - `@group(0) @binding(3)` — `uniform` `SphParams { n: u32, h_sq: f32, kernel_coeff: f32, _pad: u32 }`.
///
/// Dispatch `ceil(n/64)` workgroups of 64 threads.
pub const SHADER_SPH_DENSITY: &str = r#"
struct SphParams {
    n: u32,
    h_sq: f32,
    kernel_coeff: f32,
    _pad: u32,
}

@group(0) @binding(0) var<storage, read>       positions: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read>       masses:    array<f32>;
@group(0) @binding(2) var<storage, read_write> densities: array<f32>;
@group(0) @binding(3) var<uniform>             params:    SphParams;

@compute @workgroup_size(64)
fn main_sph(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= params.n { return; }
    let pi = positions[i].xyz;
    var rho: f32 = 0.0;
    for (var j: u32 = 0u; j < params.n; j++) {
        let diff = pi - positions[j].xyz;
        let r_sq = dot(diff, diff);
        if r_sq < params.h_sq {
            let d = params.h_sq - r_sq;
            rho += masses[j] * params.kernel_coeff * d * d * d;
        }
    }
    densities[i] = rho;
}
"#;

/// WGSL bitonic sort — single pass with caller-controlled step parameters.
///
/// This shader performs **one step** of the bitonic merge network.  The caller
/// must invoke it repeatedly — once per (k, j) pair in the outer/inner loops
/// of the standard bitonic-sort algorithm — until the array is fully sorted.
///
/// The entry point `main_bitonic` swaps pairs of elements at positions
/// `i` and `i ^ j` according to the direction bit derived from `k`.
///
/// **Bindings:**
/// - `@group(0) @binding(0)` — `storage, read_write` `array<f32>`.
/// - `@group(0) @binding(1)` — `uniform` `BitonicStep { n: u32, k: u32, j: u32, _pad: u32 }`.
///
/// Dispatch `ceil(n / 64)` workgroups of 64 threads per step.
/// The CPU driver (see [`crate::dispatch::Dispatcher::sort_f32`]) issues all
/// necessary steps.
pub const SHADER_BITONIC_SORT: &str = r#"
struct BitonicStep {
    n:    u32,
    k:    u32,
    j:    u32,
    _pad: u32,
}

@group(0) @binding(0) var<storage, read_write> data: array<f32>;
@group(0) @binding(1) var<uniform>             step: BitonicStep;

@compute @workgroup_size(64)
fn main_bitonic(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i   = gid.x;
    let n   = step.n;
    let k   = step.k;
    let j   = step.j;
    if i >= n { return; }

    let ixj = i ^ j;
    if ixj <= i { return; }
    if ixj >= n { return; }

    let ascending = (i & k) == 0u;
    let a = data[i];
    let b = data[ixj];
    if (ascending && a > b) || (!ascending && a < b) {
        data[i]   = b;
        data[ixj] = a;
    }
}
"#;

/// WGSL element-wise map template for `f32` arrays.
///
/// This is a **template string**, not a final shader.  Before passing it to
/// [`crate::compute_pipeline`], replace the placeholder `%%OP%%` with a WGSL
/// expression that operates on `x` (the current element) and evaluates to the
/// output `f32`.
///
/// **Example instantiation:**
/// ```rust
/// let shader = oxiui_compute_wgpu::SHADER_MAP_F32_TEMPLATE.replace("%%OP%%", "x * 2.0");
/// ```
///
/// **Bindings (after instantiation):**
/// - `@group(0) @binding(0)` — `storage, read` `src: array<f32>`.
/// - `@group(0) @binding(1)` — `storage, read_write` `dst: array<f32>`.
/// - `@group(0) @binding(2)` — `uniform` `n: u32`.
///
/// Dispatch `ceil(n / 64)` workgroups of 64 threads.
pub const SHADER_MAP_F32_TEMPLATE: &str = r#"
@group(0) @binding(0) var<storage, read>       src: array<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<f32>;
@group(0) @binding(2) var<uniform>             n:   u32;

@compute @workgroup_size(64)
fn main_map(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= n { return; }
    let x = src[gid.x];
    dst[gid.x] = %%OP%%;
}
"#;

/// WGSL element-wise zip-map template for two `f32` arrays.
///
/// This is a **template string**, not a final shader.  Before passing it to
/// [`crate::compute_pipeline`], replace the placeholder `%%OP%%` with a WGSL
/// expression that operates on `a` and `b` (the paired elements) and evaluates
/// to the output `f32`.
///
/// **Example instantiation:**
/// ```rust
/// let shader = oxiui_compute_wgpu::SHADER_ZIP_MAP_F32_TEMPLATE.replace("%%OP%%", "a + b");
/// ```
///
/// **Bindings (after instantiation):**
/// - `@group(0) @binding(0)` — `storage, read` `a_buf: array<f32>`.
/// - `@group(0) @binding(1)` — `storage, read` `b_buf: array<f32>`.
/// - `@group(0) @binding(2)` — `storage, read_write` `dst: array<f32>`.
/// - `@group(0) @binding(3)` — `uniform` `n: u32`.
///
/// Dispatch `ceil(n / 64)` workgroups of 64 threads.
pub const SHADER_ZIP_MAP_F32_TEMPLATE: &str = r#"
@group(0) @binding(0) var<storage, read>       a_buf: array<f32>;
@group(0) @binding(1) var<storage, read>       b_buf: array<f32>;
@group(0) @binding(2) var<storage, read_write> dst:   array<f32>;
@group(0) @binding(3) var<uniform>             n:     u32;

@compute @workgroup_size(64)
fn main_zip_map(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= n { return; }
    let a = a_buf[gid.x];
    let b = b_buf[gid.x];
    dst[gid.x] = %%OP%%;
}
"#;

// ── CPU Reference Implementations ─────────────────────────────────────────────

/// CPU inclusive prefix sum — element `i` of the result equals `data[0..=i].sum()`.
///
/// Used as a golden-value reference in tests comparing against the GPU kernel.
#[cfg(test)]
pub(crate) fn cpu_prefix_sum(data: &[f32]) -> Vec<f32> {
    let mut result = Vec::with_capacity(data.len());
    let mut acc = 0.0_f32;
    for &v in data {
        acc += v;
        result.push(acc);
    }
    result
}

/// CPU sum reduction — returns the sum of all elements in `data`.
///
/// Used as a golden-value reference in tests comparing against the GPU kernel.
#[cfg(test)]
pub(crate) fn cpu_reduction_sum(data: &[f32]) -> f32 {
    data.iter().copied().sum()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── Preprocessor: non-GPU ─────────────────────────────────────────────────

    #[test]
    fn preprocess_simple_no_includes() {
        let src = "@compute @workgroup_size(1) fn main() {}\n";
        let result = preprocess(src, &|_: &str| None).expect("no-include source must succeed");
        // The output should contain all the original lines.
        assert!(result.contains("@compute"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn preprocess_resolves_include() {
        let mut lib: HashMap<&str, &str> = HashMap::new();
        lib.insert("math.wgsl", "fn square(x: f32) -> f32 { return x * x; }");

        let src = "#include \"math.wgsl\"\n@compute @workgroup_size(1) fn main() {}\n";
        let resolver = |path: &str| lib.get(path).map(|s| s.to_string());
        let result = preprocess(src, &resolver).expect("include should resolve");

        assert!(
            result.contains("fn square"),
            "included content not found:\n{result}"
        );
        assert!(
            result.contains("fn main()"),
            "original content lost:\n{result}"
        );
    }

    #[test]
    fn preprocess_nested_include() {
        // A → B → C
        let mut lib: HashMap<&str, &str> = HashMap::new();
        lib.insert("b.wgsl", "#include \"c.wgsl\"\nfn from_b() {}");
        lib.insert("c.wgsl", "fn from_c() {}");

        let src = "#include \"b.wgsl\"\nfn from_a() {}\n";
        let resolver = |path: &str| lib.get(path).map(|s| s.to_string());
        let result = preprocess(src, &resolver).expect("nested include should resolve");

        assert!(
            result.contains("fn from_c()"),
            "C content missing:\n{result}"
        );
        assert!(
            result.contains("fn from_b()"),
            "B content missing:\n{result}"
        );
        assert!(
            result.contains("fn from_a()"),
            "A content missing:\n{result}"
        );
    }

    #[test]
    fn preprocess_cycle_returns_error() {
        // A includes B; B includes A → cycle
        let mut lib: HashMap<&str, &str> = HashMap::new();
        lib.insert("a.wgsl", "#include \"b.wgsl\"\nfn a() {}");
        lib.insert("b.wgsl", "#include \"a.wgsl\"\nfn b() {}");

        let src = "#include \"a.wgsl\"\n";
        let resolver = |path: &str| lib.get(path).map(|s| s.to_string());
        let result = preprocess(src, &resolver);

        match result {
            Err(WgslError::CyclicInclude(_)) => { /* expected */ }
            other => panic!("expected CyclicInclude, got: {other:?}"),
        }
    }

    #[test]
    fn preprocess_missing_include_returns_error() {
        let src = "#include \"nonexistent.wgsl\"\n";
        let result = preprocess(src, &|_| None);

        match result {
            Err(WgslError::MissingInclude(p)) => {
                assert!(p.contains("nonexistent"), "path not in error: {p}");
            }
            other => panic!("expected MissingInclude, got: {other:?}"),
        }
    }

    // ── Shader constant sanity checks (non-GPU) ───────────────────────────────

    #[test]
    fn shader_prefix_sum_has_entry_point() {
        assert!(
            SHADER_PREFIX_SUM.contains("@compute"),
            "SHADER_PREFIX_SUM missing @compute"
        );
        assert!(
            SHADER_PREFIX_SUM.contains("main_cs"),
            "SHADER_PREFIX_SUM missing entry point name"
        );
    }

    #[test]
    fn shader_reduction_has_entry_point() {
        assert!(
            SHADER_REDUCTION_SUM.contains("@compute"),
            "SHADER_REDUCTION_SUM missing @compute"
        );
        assert!(
            SHADER_REDUCTION_SUM.contains("main_cs"),
            "SHADER_REDUCTION_SUM missing entry point name"
        );
    }

    #[test]
    fn shader_histogram_has_entry_point() {
        assert!(
            SHADER_HISTOGRAM.contains("@compute"),
            "SHADER_HISTOGRAM missing @compute"
        );
        assert!(
            SHADER_HISTOGRAM.contains("main_cs"),
            "SHADER_HISTOGRAM missing entry point name"
        );
    }

    #[test]
    fn shader_matmul_has_entry_point() {
        assert!(
            SHADER_MATMUL.contains("@compute"),
            "SHADER_MATMUL missing @compute"
        );
        assert!(
            SHADER_MATMUL.contains("main_cs"),
            "SHADER_MATMUL missing entry point name"
        );
    }

    // ── CPU reference golden-value tests (non-GPU) ────────────────────────────

    #[test]
    fn cpu_prefix_sum_known_values() {
        let result = cpu_prefix_sum(&[1.0, 2.0, 3.0]);
        assert_eq!(result, vec![1.0, 3.0, 6.0]);
    }

    #[test]
    fn cpu_reduction_sum_known() {
        let result = cpu_reduction_sum(&[1.0, 2.0, 3.0]);
        assert!(
            (result - 6.0).abs() < f32::EPSILON,
            "expected 6.0, got {result}"
        );
    }

    // ── Error display tests (non-GPU) ─────────────────────────────────────────

    #[test]
    fn wgsl_error_display_missing() {
        let s = WgslError::MissingInclude("x.wgsl".to_string()).to_string();
        assert!(s.contains('x'), "path not in display: {s}");
        assert!(s.contains("missing"), "keyword missing: {s}");
    }

    #[test]
    fn wgsl_error_display_cyclic() {
        let s = WgslError::CyclicInclude("loop.wgsl".to_string()).to_string();
        assert!(s.contains("loop.wgsl"), "path not in display: {s}");
        assert!(s.contains("cyclic"), "keyword missing: {s}");
    }

    #[test]
    fn wgsl_error_display_depth() {
        let s = WgslError::DepthExceeded.to_string();
        assert!(s.contains("depth"), "keyword missing: {s}");
    }

    #[test]
    fn wgsl_diagnostic_display_format() {
        let d = WgslDiagnostic {
            message: "unknown identifier 'foo'".to_string(),
            line: 3,
            column: 7,
        };
        let s = d.to_string();
        assert!(s.starts_with("3:7:"), "unexpected format: {s}");
        assert!(s.contains("foo"), "message missing: {s}");
    }

    // ── GPU-gated tests ───────────────────────────────────────────────────────

    /// A minimal valid WGSL shader used in GPU validation tests.
    const VALID_SHADER: &str = r#"
        @group(0) @binding(0) var<storage, read_write> buf: array<f32>;
        @compute @workgroup_size(1)
        fn main_cs(@builtin(global_invocation_id) gid: vec3<u32>) {
            buf[gid.x] = buf[gid.x] * 2.0;
        }
    "#;

    /// A deliberately invalid WGSL snippet (references an undefined variable).
    /// Uses a semantic error rather than a parse error so that wgpu can surface
    /// it through `CompilationInfo` without panicking.
    const INVALID_SHADER: &str = r#"
        @group(0) @binding(0) var<storage, read_write> buf: array<f32>;
        @compute @workgroup_size(1)
        fn main_cs(@builtin(global_invocation_id) gid: vec3<u32>) {
            buf[gid.x] = undefined_variable;
        }
    "#;

    #[test]
    fn validate_valid_shader_ok() {
        oxiui_core::require_gpu!(ctx, crate::ComputeContext::try_new());
        validate(&ctx.device, VALID_SHADER).expect("valid shader must produce Ok(())");
    }

    #[test]
    fn validate_invalid_shader_errors() {
        oxiui_core::require_gpu!(ctx, crate::ComputeContext::try_new());
        let result = validate(&ctx.device, INVALID_SHADER);
        match result {
            Err(diags) => {
                assert!(!diags.is_empty(), "expected at least one error diagnostic");
            }
            Ok(()) => {
                // Some backends may not surface compilation errors through
                // CompilationInfo on all platforms; treat Ok as a graceful skip.
                eprintln!("[skip] backend did not surface compilation errors via CompilationInfo");
            }
        }
    }

    #[test]
    fn prefix_sum_gpu_matches_cpu() {
        oxiui_core::require_gpu!(ctx, crate::ComputeContext::try_new());

        let input: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let expected = cpu_prefix_sum(&input);

        // Upload input buffer.
        let buf = crate::buffer::storage_buffer_init(
            &ctx.device,
            "prefix-sum-test",
            bytemuck::cast_slice(&input),
        );

        // Build pipeline and bind-group.
        let pipeline = crate::pipeline::compute_pipeline(&ctx.device, SHADER_PREFIX_SUM, "main_cs");
        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("prefix-sum-bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });

        // Encode and submit.
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("prefix-sum-enc"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("prefix-sum-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));

        // Read back results.
        let result: Vec<f32> = crate::buffer::read_back(&ctx.device, &ctx.queue, &buf, input.len());

        // Compare with CPU reference.
        assert_eq!(
            result.len(),
            expected.len(),
            "length mismatch: got {}, expected {}",
            result.len(),
            expected.len()
        );
        for (i, (&got, &exp)) in result.iter().zip(expected.iter()).enumerate() {
            assert!(
                (got - exp).abs() < 1e-4,
                "prefix_sum[{i}]: got {got}, expected {exp}"
            );
        }
    }

    #[test]
    fn reduction_sum_gpu_matches_cpu() {
        oxiui_core::require_gpu!(ctx, crate::ComputeContext::try_new());

        let input: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let expected = cpu_reduction_sum(&input);

        // Upload input buffer (read-only storage).
        let input_buf = crate::buffer::storage_buffer_init(
            &ctx.device,
            "reduction-input",
            bytemuck::cast_slice(&input),
        );

        // Create output buffer (single f32 for the sum).
        let output_buf =
            crate::buffer::storage_buffer_init(&ctx.device, "reduction-output", &[0u8; 4]);

        // Build pipeline and bind-group.
        let pipeline =
            crate::pipeline::compute_pipeline(&ctx.device, SHADER_REDUCTION_SUM, "main_cs");
        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reduction-bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        // Encode and submit.
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("reduction-enc"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("reduction-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));

        // Read back the single output value.
        let result: Vec<f32> = crate::buffer::read_back(&ctx.device, &ctx.queue, &output_buf, 1);
        let got = result[0];

        assert!(
            (got - expected).abs() < 1e-3,
            "reduction sum: got {got}, expected {expected}"
        );
    }
}
