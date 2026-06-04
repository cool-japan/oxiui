//! Prefix-sum example for oxiui-compute-wgpu.
//!
//! Run: `cargo run --example prefix_sum -p oxiui-compute-wgpu`
//! Requires a GPU. Prints a skip message and exits cleanly without one.

use oxiui_compute_wgpu::{
    bytemuck, compute_pipeline, dispatch_1d, read_back, storage_buffer_init, wgpu, ComputeContext,
    SHADER_PREFIX_SUM,
};

fn main() {
    let Some(ctx) = ComputeContext::try_new() else {
        println!("[skip] No GPU adapter found — prefix_sum example requires a GPU.");
        return;
    };

    println!("Adapter: {:?}", ctx.adapter_info().backend);

    // Input: [1, 2, 3, 4, 5, 6, 7, 8]
    let input: Vec<f32> = (1..=8).map(|x| x as f32).collect();
    let expected: Vec<f32> = {
        let mut acc = 0.0f32;
        input
            .iter()
            .map(|&v| {
                acc += v;
                acc
            })
            .collect()
    };

    // Upload to GPU storage buffer
    let buf = storage_buffer_init(&ctx.device, "prefix-sum", bytemuck::cast_slice(&input));

    // Compile the prefix-sum shader
    let pipeline = compute_pipeline(&ctx.device, SHADER_PREFIX_SUM, "main_cs");

    // Bind group
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("prefix-sum-bg"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });

    // Dispatch: single workgroup covers up to 256 elements
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("prefix-sum-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        // dispatch_1d uses ceiling division; for a workgroup_size=256 kernel and
        // n<=256 elements, one workgroup is always correct.
        let _gx = dispatch_1d(input.len() as u32, 256);
        pass.dispatch_workgroups(1, 1, 1);
    }
    ctx.queue.submit(std::iter::once(encoder.finish()));

    // Read back
    let output: Vec<f32> = read_back(&ctx.device, &ctx.queue, &buf, input.len());

    println!("Input:    {:?}", input);
    println!("Expected: {:?}", expected);
    println!("Output:   {:?}", output);

    // Validate
    let ok = output
        .iter()
        .zip(expected.iter())
        .all(|(a, b)| (a - b).abs() < 1e-3);
    if ok {
        println!("OK — prefix sum matches CPU reference.");
    } else {
        println!("MISMATCH — GPU result differs from CPU reference!");
        std::process::exit(1);
    }
}
