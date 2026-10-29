#include <jni.h>
#include <opus.h>

namespace {

constexpr int kSampleRate = 48000;
constexpr int kChannels = 2;
constexpr int kFrame = 480;

inline OpusEncoder *encoder(jlong handle) {
    return reinterpret_cast<OpusEncoder *>(handle);
}

inline OpusDecoder *decoder(jlong handle) {
    return reinterpret_cast<OpusDecoder *>(handle);
}

}

extern "C" JNIEXPORT jlong JNICALL
Java_com_tchirek_locosaonduh_engine_NativeOpus_createEncoder(JNIEnv *, jclass) {
    int error = OPUS_OK;
    OpusEncoder *encoder = opus_encoder_create(kSampleRate, kChannels,
                                               OPUS_APPLICATION_AUDIO, &error);
    if (error != OPUS_OK || encoder == nullptr) {
        return 0;
    }
    opus_encoder_ctl(encoder, OPUS_SET_COMPLEXITY(10));
    opus_encoder_ctl(encoder, OPUS_SET_VBR(1));
    opus_encoder_ctl(encoder, OPUS_SET_VBR_CONSTRAINT(1));
    opus_encoder_ctl(encoder, OPUS_SET_SIGNAL(OPUS_SIGNAL_MUSIC));
    opus_encoder_ctl(encoder, OPUS_SET_BITRATE(128000));
    opus_encoder_ctl(encoder, OPUS_SET_PACKET_LOSS_PERC(1));
    return reinterpret_cast<jlong>(encoder);
}

extern "C" JNIEXPORT jlong JNICALL
Java_com_tchirek_locosaonduh_engine_NativeOpus_createDecoder(JNIEnv *, jclass) {
    int error = OPUS_OK;
    OpusDecoder *decoder = opus_decoder_create(kSampleRate, kChannels, &error);
    if (error != OPUS_OK || decoder == nullptr) {
        return 0;
    }
    return reinterpret_cast<jlong>(decoder);
}

extern "C" JNIEXPORT void JNICALL
Java_com_tchirek_locosaonduh_engine_NativeOpus_configure(JNIEnv *, jclass,
                                                         jlong handle,
                                                         jint bitrateKbps,
                                                         jint lossPercent) {
    if (handle == 0) {
        return;
    }
    int bitrate = bitrateKbps < 32 ? 32 : (bitrateKbps > 256 ? 256 : bitrateKbps);
    int loss = lossPercent < 0 ? 0 : (lossPercent > 30 ? 30 : lossPercent);
    opus_encoder_ctl(encoder(handle), OPUS_SET_BITRATE(bitrate * 1000));
    opus_encoder_ctl(encoder(handle), OPUS_SET_PACKET_LOSS_PERC(loss));
}

extern "C" JNIEXPORT jint JNICALL
Java_com_tchirek_locosaonduh_engine_NativeOpus_encode(JNIEnv *env, jclass,
                                                      jlong handle,
                                                      jfloatArray pcm,
                                                      jbyteArray output,
                                                      jint capacity) {
    if (handle == 0) {
        return -1;
    }
    jfloat samples[kFrame * kChannels];
    env->GetFloatArrayRegion(pcm, 0, kFrame * kChannels, samples);
    if (env->ExceptionCheck()) {
        return -1;
    }
    unsigned char packet[1500];
    int limit = capacity < 1500 ? capacity : 1500;
    int written = opus_encode_float(encoder(handle), samples, kFrame, packet, limit);
    if (written <= 0) {
        return written < 0 ? written : -1;
    }
    env->SetByteArrayRegion(output, 0, written, reinterpret_cast<jbyte *>(packet));
    return written;
}

extern "C" JNIEXPORT jint JNICALL
Java_com_tchirek_locosaonduh_engine_NativeOpus_decode(JNIEnv *env, jclass,
                                                      jlong handle,
                                                      jbyteArray payload,
                                                      jfloatArray pcm) {
    if (handle == 0) {
        return -1;
    }
    jfloat samples[kFrame * kChannels];
    jsize length = payload == nullptr ? 0 : env->GetArrayLength(payload);
    jbyte packet[1500];
    if (length > 0) {
        env->GetByteArrayRegion(payload, 0, length, packet);
        if (env->ExceptionCheck()) {
            return -1;
        }
    }
    int decoded = opus_decode_float(
        decoder(handle),
        length > 0 ? reinterpret_cast<const unsigned char *>(packet) : nullptr,
        length > 0 ? length : 0, samples, kFrame, 0);
    if (decoded < 0) {
        return decoded;
    }
    env->SetFloatArrayRegion(pcm, 0, kFrame * kChannels, samples);
    return decoded;
}

extern "C" JNIEXPORT void JNICALL
Java_com_tchirek_locosaonduh_engine_NativeOpus_destroy(JNIEnv *, jclass,
                                                       jlong handle) {
    if (handle == 0) {
        return;
    }
    opus_encoder_destroy(encoder(handle));
}
