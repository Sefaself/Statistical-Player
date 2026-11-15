//! Real-time safety test for the audio callback.
//! Verifies that render_callback_f32, render_callback_i16, and render_callback_i32
//! perform zero heap allocations (using a custom global allocator counting allocations).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use statistical_player::audio::sink::{
    CallbackShared, render_callback_f32, render_callback_i16, render_callback_i32,
};

struct CountingAllocator;

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOC: CountingAllocator = CountingAllocator;

#[test]
fn test_audio_callback_zero_allocations() {
    // 1. Setup ring buffer and shared state (before allocation counting begins)
    let (mut producer, mut consumer) = rtrb::RingBuffer::new(2048);
    let shared = CallbackShared::default();
    let mut current_gain = 1.0_f32;
    let ramp_rate = 0.002_f32;

    // Pre-fill ring buffer with 512 stereo samples
    for i in 0..512 {
        let _ = producer.push((i as f32) * 0.001);
    }

    let mut output_f32 = [0.0_f32; 256];
    let mut output_i16 = [0_i16; 256];
    let mut output_i32 = [0_i32; 256];

    // Warm-up run
    render_callback_f32(
        &mut output_f32,
        &mut consumer,
        &shared,
        &mut current_gain,
        ramp_rate,
    );

    // Refill for measured run
    for i in 0..256 {
        let _ = producer.push((i as f32) * 0.001);
    }

    // 2. Reset allocation counter
    ALLOC_COUNT.store(0, Ordering::SeqCst);

    // 3. Execute f32 callback
    render_callback_f32(
        &mut output_f32,
        &mut consumer,
        &shared,
        &mut current_gain,
        ramp_rate,
    );
    let f32_allocs = ALLOC_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        f32_allocs, 0,
        "render_callback_f32 allocated on heap ({f32_allocs} allocations)"
    );

    // 4. Test underrun path (when buffer is empty)
    render_callback_f32(
        &mut output_f32,
        &mut consumer,
        &shared,
        &mut current_gain,
        ramp_rate,
    );
    let underrun_allocs = ALLOC_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        underrun_allocs, 0,
        "render_callback_f32 underrun path allocated on heap ({underrun_allocs} allocations)"
    );

    // 5. Test i16 callback
    for i in 0..256 {
        let _ = producer.push((i as f32) * 0.001);
    }
    ALLOC_COUNT.store(0, Ordering::SeqCst);
    render_callback_i16(
        &mut output_i16,
        &mut consumer,
        &shared,
        &mut current_gain,
        ramp_rate,
    );
    let i16_allocs = ALLOC_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        i16_allocs, 0,
        "render_callback_i16 allocated on heap ({i16_allocs} allocations)"
    );

    // 6. Test i32 callback
    for i in 0..256 {
        let _ = producer.push((i as f32) * 0.001);
    }
    ALLOC_COUNT.store(0, Ordering::SeqCst);
    render_callback_i32(
        &mut output_i32,
        &mut consumer,
        &shared,
        &mut current_gain,
        ramp_rate,
    );
    let i32_allocs = ALLOC_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        i32_allocs, 0,
        "render_callback_i32 allocated on heap ({i32_allocs} allocations)"
    );

    // 7. Test discard / flush path (zero allocations when draining old samples)
    for i in 0..256 {
        let _ = producer.push((i as f32) * 0.001);
    }
    let consumed_now = shared.frames_consumed.load(Ordering::Relaxed);
    shared
        .discard_target
        .store(consumed_now + 128, Ordering::Relaxed);
    ALLOC_COUNT.store(0, Ordering::SeqCst);
    render_callback_f32(
        &mut output_f32,
        &mut consumer,
        &shared,
        &mut current_gain,
        ramp_rate,
    );
    let discard_allocs = ALLOC_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        discard_allocs, 0,
        "drain_discard path allocated on heap ({discard_allocs} allocations)"
    );
}
