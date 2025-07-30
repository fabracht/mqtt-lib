use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use std::time::Duration;

// Import our new BeBytes 2.4.0 implementations
use bebytes::BeBytes;
use bytes::BufMut;
use mqtt_v5::encoding::mqtt_string::MqttString;
use mqtt_v5::encoding::variable_int::VariableInt;

// We'll create modules that encapsulate the old (manual) and new (bebytes) implementations
mod manual_impl {
    use bytes::{Buf, BufMut};

    // Manual bit manipulation for fixed header (pre-bebytes approach)
    pub fn encode_fixed_header_manual(buf: &mut impl BufMut, packet_type: u8, flags: u8) {
        let first_byte = (packet_type << 4) | (flags & 0x0F);
        buf.put_u8(first_byte);
    }

    pub fn decode_fixed_header_manual(buf: &mut impl Buf) -> (u8, u8) {
        let first_byte = buf.get_u8();
        let packet_type = first_byte >> 4;
        let flags = first_byte & 0x0F;
        (packet_type, flags)
    }

    // Manual subscription options encoding
    pub fn encode_subscription_options_manual(
        buf: &mut impl BufMut,
        qos: u8,
        no_local: bool,
        retain_as_published: bool,
        retain_handling: u8,
    ) {
        let byte = (qos & 0x03)
            | ((no_local as u8) << 2)
            | ((retain_as_published as u8) << 3)
            | ((retain_handling & 0x03) << 4);
        buf.put_u8(byte);
    }

    pub fn decode_subscription_options_manual(buf: &mut impl Buf) -> (u8, bool, bool, u8) {
        let byte = buf.get_u8();
        let qos = byte & 0x03;
        let no_local = (byte & 0x04) != 0;
        let retain_as_published = (byte & 0x08) != 0;
        let retain_handling = (byte >> 4) & 0x03;
        (qos, no_local, retain_as_published, retain_handling)
    }

    // Manual PingReq packet
    pub struct PingReqManual;

    impl PingReqManual {
        pub fn encode(&self, buf: &mut impl BufMut) {
            buf.put_u8(0xC0); // PINGREQ packet type
            buf.put_u8(0x00); // Remaining length
        }

        pub fn decode(buf: &mut impl Buf) -> Self {
            let _packet_type = buf.get_u8();
            let _remaining_len = buf.get_u8();
            Self
        }
    }

    // Manual acknowledgment header
    pub fn encode_ack_header_manual(buf: &mut impl BufMut, packet_id: u16, reason_code: u8) {
        buf.put_u16(packet_id);
        buf.put_u8(reason_code);
    }

    pub fn decode_ack_header_manual(buf: &mut impl Buf) -> (u16, u8) {
        let packet_id = buf.get_u16();
        let reason_code = buf.get_u8();
        (packet_id, reason_code)
    }
}

mod bebytes_impl {
    use bebytes::BeBytes;
    use bytes::{Buf, BufMut};

    // BeBytes implementation for type and flags
    #[derive(Debug, Clone, Copy, BeBytes)]
    pub struct MqttTypeAndFlags {
        #[bits(4)]
        pub message_type: u8,
        #[bits(1)]
        pub dup: u8,
        #[bits(2)]
        pub qos: u8,
        #[bits(1)]
        pub retain: u8,
    }

    impl MqttTypeAndFlags {
        pub fn create(packet_type: u8, flags: u8) -> Self {
            Self {
                message_type: packet_type,
                dup: (flags >> 3) & 0x01,
                qos: (flags >> 1) & 0x03,
                retain: flags & 0x01,
            }
        }

        pub fn encode(&self, buf: &mut impl BufMut) {
            let bytes = self.to_be_bytes();
            buf.put_slice(&bytes);
        }

        pub fn decode(buf: &mut impl Buf) -> Self {
            let byte = buf.get_u8();
            let (result, _) = Self::try_from_be_bytes(&[byte]).unwrap();
            result
        }
    }

    // BeBytes subscription options
    #[derive(Debug, Clone, Copy, BeBytes)]
    pub struct SubscriptionOptionsBits {
        #[bits(2)]
        pub reserved_bits: u8,
        #[bits(2)]
        pub retain_handling: u8,
        #[bits(1)]
        pub retain_as_published: u8,
        #[bits(1)]
        pub no_local: u8,
        #[bits(2)]
        pub qos: u8,
    }

    impl SubscriptionOptionsBits {
        pub fn create(
            qos: u8,
            no_local: bool,
            retain_as_published: bool,
            retain_handling: u8,
        ) -> Self {
            Self {
                reserved_bits: 0,
                retain_handling,
                retain_as_published: retain_as_published as u8,
                no_local: no_local as u8,
                qos,
            }
        }

        pub fn encode(&self, buf: &mut impl BufMut) {
            let bytes = self.to_be_bytes();
            buf.put_slice(&bytes);
        }

        pub fn decode(buf: &mut impl Buf) -> Self {
            let byte = buf.get_u8();
            let (result, _) = Self::try_from_be_bytes(&[byte]).unwrap();
            result
        }
    }

    // BeBytes PingReq packet
    #[derive(Debug, Clone, Copy, BeBytes)]
    pub struct PingReqBebytes {
        #[bebytes(big_endian)]
        fixed_header: u16,
    }

    impl PingReqBebytes {
        pub fn create() -> Self {
            Self {
                fixed_header: 0xC000,
            }
        }

        pub fn encode(&self, buf: &mut impl BufMut) {
            let bytes = self.to_be_bytes();
            buf.put_slice(&bytes);
        }

        pub fn decode(buf: &mut impl Buf) -> Self {
            let mut bytes = [0u8; 2];
            buf.copy_to_slice(&mut bytes);
            let (result, _) = Self::try_from_be_bytes(&bytes).unwrap();
            result
        }
    }

    // BeBytes acknowledgment header
    #[derive(Debug, Clone, Copy, BeBytes)]
    pub struct AckPacketHeader {
        #[bebytes(big_endian)]
        pub packet_id: u16,
        pub reason_code: u8,
    }

    impl AckPacketHeader {
        pub fn create(packet_id: u16, reason_code: u8) -> Self {
            Self {
                packet_id,
                reason_code,
            }
        }

        pub fn encode(&self, buf: &mut impl BufMut) {
            let bytes = self.to_be_bytes();
            buf.put_slice(&bytes);
        }

        pub fn decode(buf: &mut impl Buf) -> Self {
            let mut bytes = [0u8; 3];
            buf.copy_to_slice(&mut bytes);
            let (result, _) = Self::try_from_be_bytes(&bytes).unwrap();
            result
        }
    }
}

// Benchmark functions
fn bench_fixed_header_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixed_header_encoding");

    // Manual implementation
    group.bench_function("manual", |b| {
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);

        b.iter(|| {
            buf.clear();
            manual_impl::encode_fixed_header_manual(&mut buf, 3, 0b0010); // PUBLISH with QoS 1
            black_box(&buf);
        });
    });

    // BeBytes implementation
    group.bench_function("bebytes", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            buf.clear();
            header.encode(&mut buf);
            black_box(&buf);
        });
    });

    group.finish();
}

fn bench_fixed_header_decoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixed_header_decoding");

    // Manual implementation
    group.bench_function("manual", |b| {
        use bytes::{BufMut, BytesMut};
        let mut buf = BytesMut::new();
        buf.put_u8(0x32); // PUBLISH with flags
        let data = buf.freeze();

        b.iter(|| {
            let mut buf = data.clone();
            let result = manual_impl::decode_fixed_header_manual(&mut buf);
            black_box(result);
        });
    });

    // BeBytes implementation
    group.bench_function("bebytes", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::{BufMut, BytesMut};
        let mut buf = BytesMut::new();
        buf.put_u8(0x32); // PUBLISH with flags
        let data = buf.freeze();

        b.iter(|| {
            let mut buf = data.clone();
            let result = MqttTypeAndFlags::decode(&mut buf);
            black_box(result);
        });
    });

    group.finish();
}

fn bench_subscription_options(c: &mut Criterion) {
    let mut group = c.benchmark_group("subscription_options");

    // Encoding benchmark
    group.bench_function("encode_manual", |b| {
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);

        b.iter(|| {
            buf.clear();
            manual_impl::encode_subscription_options_manual(&mut buf, 1, true, false, 2);
            black_box(&buf);
        });
    });

    group.bench_function("encode_bebytes", |b| {
        use bebytes_impl::SubscriptionOptionsBits;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let options = SubscriptionOptionsBits::create(1, true, false, 2);

        b.iter(|| {
            buf.clear();
            options.encode(&mut buf);
            black_box(&buf);
        });
    });

    // Decoding benchmark
    group.bench_function("decode_manual", |b| {
        use bytes::{BufMut, BytesMut};
        let mut buf = BytesMut::new();
        buf.put_u8(0b00100101); // Sample subscription options
        let data = buf.freeze();

        b.iter(|| {
            let mut buf = data.clone();
            let result = manual_impl::decode_subscription_options_manual(&mut buf);
            black_box(result);
        });
    });

    group.bench_function("decode_bebytes", |b| {
        use bebytes_impl::SubscriptionOptionsBits;
        use bytes::{BufMut, BytesMut};
        let mut buf = BytesMut::new();
        buf.put_u8(0b00100101); // Sample subscription options
        let data = buf.freeze();

        b.iter(|| {
            let mut buf = data.clone();
            let result = SubscriptionOptionsBits::decode(&mut buf);
            black_box(result);
        });
    });

    group.finish();
}

fn bench_pingreq_packet(c: &mut Criterion) {
    let mut group = c.benchmark_group("pingreq_packet");

    // Encoding benchmark
    group.bench_function("encode_manual", |b| {
        use bytes::BytesMut;
        use manual_impl::PingReqManual;
        let mut buf = BytesMut::with_capacity(16);
        let packet = PingReqManual;

        b.iter(|| {
            buf.clear();
            packet.encode(&mut buf);
            black_box(&buf);
        });
    });

    group.bench_function("encode_bebytes", |b| {
        use bebytes_impl::PingReqBebytes;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let packet = PingReqBebytes::create();

        b.iter(|| {
            buf.clear();
            packet.encode(&mut buf);
            black_box(&buf);
        });
    });

    // Decoding benchmark
    group.bench_function("decode_manual", |b| {
        use bytes::{BufMut, BytesMut};
        use manual_impl::PingReqManual;
        let mut buf = BytesMut::new();
        buf.put_u8(0xC0);
        buf.put_u8(0x00);
        let data = buf.freeze();

        b.iter(|| {
            let mut buf = data.clone();
            let result = PingReqManual::decode(&mut buf);
            black_box(result);
        });
    });

    group.bench_function("decode_bebytes", |b| {
        use bebytes_impl::PingReqBebytes;
        use bytes::{BufMut, BytesMut};
        let mut buf = BytesMut::new();
        buf.put_u8(0xC0);
        buf.put_u8(0x00);
        let data = buf.freeze();

        b.iter(|| {
            let mut buf = data.clone();
            let result = PingReqBebytes::decode(&mut buf);
            black_box(result);
        });
    });

    group.finish();
}

fn bench_ack_header(c: &mut Criterion) {
    let mut group = c.benchmark_group("ack_header");

    // Encoding benchmark
    group.bench_function("encode_manual", |b| {
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);

        b.iter(|| {
            buf.clear();
            manual_impl::encode_ack_header_manual(&mut buf, 12345, 0x00);
            black_box(&buf);
        });
    });

    group.bench_function("encode_bebytes", |b| {
        use bebytes_impl::AckPacketHeader;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let header = AckPacketHeader::create(12345, 0x00);

        b.iter(|| {
            buf.clear();
            header.encode(&mut buf);
            black_box(&buf);
        });
    });

    // Decoding benchmark
    group.bench_function("decode_manual", |b| {
        use bytes::{BufMut, BytesMut};
        let mut buf = BytesMut::new();
        buf.put_u16(12345);
        buf.put_u8(0x00);
        let data = buf.freeze();

        b.iter(|| {
            let mut buf = data.clone();
            let result = manual_impl::decode_ack_header_manual(&mut buf);
            black_box(result);
        });
    });

    group.bench_function("decode_bebytes", |b| {
        use bebytes_impl::AckPacketHeader;
        use bytes::{BufMut, BytesMut};
        let mut buf = BytesMut::new();
        buf.put_u16(12345);
        buf.put_u8(0x00);
        let data = buf.freeze();

        b.iter(|| {
            let mut buf = data.clone();
            let result = AckPacketHeader::decode(&mut buf);
            black_box(result);
        });
    });

    group.finish();
}

fn bench_round_trip_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("round_trip");

    // Fixed header round trip
    group.bench_function("fixed_header_manual", |b| {
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);

        b.iter(|| {
            buf.clear();
            manual_impl::encode_fixed_header_manual(&mut buf, 3, 0b0010);
            let (packet_type, flags) = manual_impl::decode_fixed_header_manual(&mut buf);
            black_box((packet_type, flags));
        });
    });

    group.bench_function("fixed_header_bebytes", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);

        b.iter(|| {
            buf.clear();
            let header = MqttTypeAndFlags::create(3, 0b0010);
            header.encode(&mut buf);
            let decoded = MqttTypeAndFlags::decode(&mut buf);
            black_box(decoded);
        });
    });

    // Subscription options round trip
    group.bench_function("subscription_manual", |b| {
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);

        b.iter(|| {
            buf.clear();
            manual_impl::encode_subscription_options_manual(&mut buf, 1, true, false, 2);
            let result = manual_impl::decode_subscription_options_manual(&mut buf);
            black_box(result);
        });
    });

    group.bench_function("subscription_bebytes", |b| {
        use bebytes_impl::SubscriptionOptionsBits;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);

        b.iter(|| {
            buf.clear();
            let options = SubscriptionOptionsBits::create(1, true, false, 2);
            options.encode(&mut buf);
            let decoded = SubscriptionOptionsBits::decode(&mut buf);
            black_box(decoded);
        });
    });

    group.finish();
}

// Benchmark for memory allocation patterns
fn bench_memory_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_patterns");
    group.measurement_time(Duration::from_secs(10));

    // Repeated allocations - manual
    group.bench_function("repeated_alloc_manual", |b| {
        use bytes::BytesMut;

        b.iter(|| {
            for _ in 0..100 {
                let mut buf = BytesMut::with_capacity(4);
                manual_impl::encode_fixed_header_manual(&mut buf, 3, 0b0010);
                black_box(buf);
            }
        });
    });

    // Repeated allocations - bebytes
    group.bench_function("repeated_alloc_bebytes", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;

        b.iter(|| {
            for _ in 0..100 {
                let mut buf = BytesMut::with_capacity(4);
                let header = MqttTypeAndFlags::create(3, 0b0010);
                header.encode(&mut buf);
                black_box(buf);
            }
        });
    });

    // Reused buffer - manual
    group.bench_function("reused_buffer_manual", |b| {
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(400);

        b.iter(|| {
            buf.clear();
            for _ in 0..100 {
                manual_impl::encode_fixed_header_manual(&mut buf, 3, 0b0010);
            }
            black_box(&buf);
        });
    });

    // Reused buffer - bebytes
    group.bench_function("reused_buffer_bebytes", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(400);

        b.iter(|| {
            buf.clear();
            for _ in 0..100 {
                let header = MqttTypeAndFlags::create(3, 0b0010);
                header.encode(&mut buf);
            }
            black_box(&buf);
        });
    });

    group.finish();
}

// Benchmarks for BeBytes 2.3.0 features
fn bench_mqtt_string_v23(c: &mut Criterion) {
    let mut group = c.benchmark_group("mqtt_string_v23");

    // Encoding benchmark
    group.bench_function("encode", |b| {
        let mqtt_str = MqttString::create("test/topic/name").unwrap();
        b.iter(|| {
            let bytes = mqtt_str.to_be_bytes();
            black_box(bytes);
        });
    });

    // Decoding benchmark
    group.bench_function("decode", |b| {
        let data = vec![
            0x00, 0x0F, b't', b'e', b's', b't', b'/', b't', b'o', b'p', b'i', b'c', b'/', b'n',
            b'a', b'm', b'e',
        ];
        b.iter(|| {
            let (mqtt_str, consumed) = MqttString::try_from_be_bytes(&data).unwrap();
            black_box((mqtt_str, consumed));
        });
    });

    // Round-trip benchmark
    group.bench_function("round_trip", |b| {
        let original = MqttString::create("sensor/temperature").unwrap();
        b.iter(|| {
            let bytes = original.to_be_bytes();
            let (decoded, _) = MqttString::try_from_be_bytes(&bytes).unwrap();
            black_box(decoded);
        });
    });

    group.finish();
}

fn bench_variable_int_v23(c: &mut Criterion) {
    let mut group = c.benchmark_group("variable_int_v23");

    // Single byte encoding
    group.bench_function("encode_single_byte", |b| {
        let var_int = VariableInt::new(127).unwrap();
        b.iter(|| {
            let bytes = var_int.to_be_bytes();
            black_box(bytes);
        });
    });

    // Two byte encoding
    group.bench_function("encode_two_bytes", |b| {
        let var_int = VariableInt::new(16_383).unwrap();
        b.iter(|| {
            let bytes = var_int.to_be_bytes();
            black_box(bytes);
        });
    });

    // Four byte encoding
    group.bench_function("encode_four_bytes", |b| {
        let var_int = VariableInt::new(268_435_455).unwrap();
        b.iter(|| {
            let bytes = var_int.to_be_bytes();
            black_box(bytes);
        });
    });

    // Decoding benchmarks
    group.bench_function("decode_single_byte", |b| {
        let data = vec![0x7F];
        b.iter(|| {
            let (var_int, consumed) = VariableInt::try_from_be_bytes(&data).unwrap();
            black_box((var_int, consumed));
        });
    });

    group.bench_function("decode_four_bytes", |b| {
        let data = vec![0xFF, 0xFF, 0xFF, 0x7F];
        b.iter(|| {
            let (var_int, consumed) = VariableInt::try_from_be_bytes(&data).unwrap();
            black_box((var_int, consumed));
        });
    });

    group.finish();
}

// Benchmarks for BeBytes 2.6.0 direct buffer codegen
fn bench_bebytes_26_direct_buffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("bebytes_26_direct_buffer");

    // to_be_bytes_buf() method - 2.3x performance improvement
    group.bench_function("fixed_header_to_bytes_buf", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            let bytes = header.to_be_bytes_buf();
            black_box(bytes);
        });
    });

    // Compare to_be_bytes_buf vs old to_be_bytes 
    group.bench_function("fixed_header_old_to_bytes", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            let bytes = header.to_be_bytes();
            black_box(bytes);
        });
    });

    // Raw pointer methods for structures (when available)
    group.bench_function("subscription_options_to_bytes_buf", |b| {
        use bebytes_impl::SubscriptionOptionsBits;
        let options = SubscriptionOptionsBits::create(1, true, false, 2);

        b.iter(|| {
            let bytes = options.to_be_bytes_buf();
            black_box(bytes);
        });
    });

    // MQTT String with direct buffer
    group.bench_function("mqtt_string_to_bytes_buf", |b| {
        let mqtt_str = MqttString::create("test/topic/name").unwrap();
        b.iter(|| {
            let bytes = mqtt_str.to_be_bytes_buf();
            black_box(bytes);
        });
    });

    // Variable Integer with direct buffer
    group.bench_function("variable_int_to_bytes_buf", |b| {
        let var_int = VariableInt::new(16_383).unwrap();
        b.iter(|| {
            let bytes = var_int.to_be_bytes_buf();
            black_box(bytes);
        });
    });

    group.finish();
}

// Benchmarks for BeBytes 2.4.0 zero-allocation encoding
fn bench_bebytes_24_zero_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("bebytes_24_zero_allocation");

    // Fixed header zero-allocation encoding
    group.bench_function("fixed_header_encode_to", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            buf.clear();
            let _ = header.encode_be_to(&mut buf);
            black_box(&buf);
        });
    });

    // Compare old allocation-based vs new zero-allocation
    group.bench_function("fixed_header_old_api", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            buf.clear();
            let bytes = header.to_be_bytes();
            buf.put_slice(&bytes);
            black_box(&buf);
        });
    });

    // Subscription options zero-allocation
    group.bench_function("subscription_options_encode_to", |b| {
        use bebytes_impl::SubscriptionOptionsBits;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let options = SubscriptionOptionsBits::create(1, true, false, 2);

        b.iter(|| {
            buf.clear();
            let _ = options.encode_be_to(&mut buf);
            black_box(&buf);
        });
    });

    // MQTT String zero-allocation encoding (v2.4.0)
    group.bench_function("mqtt_string_encode_to", |b| {
        use bytes::BytesMut;
        let mqtt_str = MqttString::create("test/topic/name").unwrap();
        let mut buf = BytesMut::with_capacity(32);

        b.iter(|| {
            buf.clear();
            let _ = mqtt_str.encode_be_to(&mut buf);
            black_box(&buf);
        });
    });

    // Variable Integer zero-allocation encoding (v2.4.0)
    group.bench_function("variable_int_encode_to", |b| {
        use bytes::BytesMut;
        let var_int = VariableInt::new(16_383).unwrap();
        let mut buf = BytesMut::with_capacity(8);

        b.iter(|| {
            buf.clear();
            let _ = var_int.encode_be_to(&mut buf);
            black_box(&buf);
        });
    });

    group.finish();
}

// Benchmark comparing all encoding approaches side by side
fn bench_encoding_comparison_v24(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoding_comparison_v24");

    // Fixed header: Manual vs BeBytes old API vs BeBytes 2.4.0 zero-allocation
    group.bench_function("manual", |b| {
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);

        b.iter(|| {
            buf.clear();
            manual_impl::encode_fixed_header_manual(&mut buf, 3, 0b0010);
            black_box(&buf);
        });
    });

    group.bench_function("bebytes_old_api", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            buf.clear();
            let bytes = header.to_be_bytes();
            buf.put_slice(&bytes);
            black_box(&buf);
        });
    });

    group.bench_function("bebytes_24_zero_alloc", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            buf.clear();
            let _ = header.encode_be_to(&mut buf);
            black_box(&buf);
        });
    });

    group.finish();
}

// Memory allocation comparison for BeBytes 2.4.0
fn bench_allocation_patterns_v24(c: &mut Criterion) {
    let mut group = c.benchmark_group("allocation_patterns_v24");
    group.measurement_time(Duration::from_secs(10));

    // Repeated allocations - BeBytes 2.4.0 zero-allocation
    group.bench_function("repeated_zero_alloc", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;

        b.iter(|| {
            for _ in 0..100 {
                let mut buf = BytesMut::with_capacity(4);
                let header = MqttTypeAndFlags::create(3, 0b0010);
                let _ = header.encode_be_to(&mut buf);
                black_box(buf);
            }
        });
    });

    // Reused buffer - BeBytes 2.4.0 zero-allocation
    group.bench_function("reused_buffer_zero_alloc", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(400);

        b.iter(|| {
            buf.clear();
            for _ in 0..100 {
                let header = MqttTypeAndFlags::create(3, 0b0010);
                let _ = header.encode_be_to(&mut buf);
            }
            black_box(&buf);
        });
    });

    group.finish();
}

// Comprehensive comparison of all BeBytes versions
fn bench_bebytes_version_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("bebytes_version_comparison");
    group.measurement_time(Duration::from_secs(10));

    // Fixed header comparison: Manual -> BeBytes old -> BeBytes 2.4.0 -> BeBytes 2.6.0
    group.bench_function("manual_fixed_header", |b| {
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);

        b.iter(|| {
            buf.clear();
            manual_impl::encode_fixed_header_manual(&mut buf, 3, 0b0010);
            black_box(&buf);
        });
    });

    group.bench_function("bebytes_old_api_fixed_header", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            buf.clear();
            let bytes = header.to_be_bytes();
            buf.put_slice(&bytes);
            black_box(&buf);
        });
    });

    group.bench_function("bebytes_24_encode_to_fixed_header", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        use bytes::BytesMut;
        let mut buf = BytesMut::with_capacity(16);
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            buf.clear();
            let _ = header.encode_be_to(&mut buf);
            black_box(&buf);
        });
    });

    group.bench_function("bebytes_26_to_bytes_buf_fixed_header", |b| {
        use bebytes_impl::MqttTypeAndFlags;
        let header = MqttTypeAndFlags::create(3, 0b0010);

        b.iter(|| {
            let bytes = header.to_be_bytes_buf();
            black_box(bytes);
        });
    });

    // MQTT String comparison across versions
    group.bench_function("mqtt_string_v23_to_be_bytes", |b| {
        let mqtt_str = MqttString::create("test/topic/name").unwrap();
        b.iter(|| {
            let bytes = mqtt_str.to_be_bytes();
            black_box(bytes);
        });
    });

    group.bench_function("mqtt_string_v24_encode_to", |b| {
        use bytes::BytesMut;
        let mqtt_str = MqttString::create("test/topic/name").unwrap();
        let mut buf = BytesMut::with_capacity(32);

        b.iter(|| {
            buf.clear();
            let _ = mqtt_str.encode_be_to(&mut buf);
            black_box(&buf);
        });
    });

    group.bench_function("mqtt_string_v26_to_bytes_buf", |b| {
        let mqtt_str = MqttString::create("test/topic/name").unwrap();
        b.iter(|| {
            let bytes = mqtt_str.to_be_bytes_buf();
            black_box(bytes);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_fixed_header_encoding,
    bench_fixed_header_decoding,
    bench_subscription_options,
    bench_pingreq_packet,
    bench_ack_header,
    bench_round_trip_operations,
    bench_memory_patterns,
    bench_mqtt_string_v23,
    bench_variable_int_v23,
    bench_bebytes_24_zero_allocation,
    bench_bebytes_26_direct_buffer,
    bench_encoding_comparison_v24,
    bench_allocation_patterns_v24,
    bench_bebytes_version_comparison
);
criterion_main!(benches);
