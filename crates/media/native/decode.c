#include "media.h"
#include <math.h>
#include <stdio.h>
#include <string.h>

struct DnDecoder {
    AVFormatContext *format;
    AVCodecContext *codec;
    AVFilterGraph *graph;
    AVFilterContext *source, *sink;
    AVPacket *packet;
    AVFrame *decoded, *filtered;
    int stream, audio, input_eof, codec_eof, pending_packet, samples, position, invalid;
    double start, end;
    char *filter;
    DnInterrupt interrupt;
    void *opaque;
};

/* 使用调用者的稳定取消句柄中断读包；回调不释放上下文，不访问 GUI。 */
static int interrupted(void *opaque) {
    DnDecoder *d = opaque;
    return d->interrupt && d->interrupt(d->opaque);
}

/* 释放部分或完整初始化的解码器，包含滤镜、帧和文件；允许空句柄。 */
void dn_decode_free(DnDecoder *d) {
    if (!d) return;
    avfilter_graph_free(&d->graph);
    avcodec_free_context(&d->codec);
    avformat_close_input(&d->format);
    av_packet_free(&d->packet);
    av_frame_free(&d->decoded);
    av_frame_free(&d->filtered);
    av_free(d->filter);
    av_free(d);
}

/* 打开指定本地流并向前寻址到入点之前，保留帧时间戳供精确裁剪。
   filter 为受控内部滤镜，audio 选择音频；初始化失败释放全部资源并填入诊断。 */
DnDecoder *dn_decode_open(const char *path, int audio, double start, double end,
    const char *filter, DnInterrupt interrupt, void *opaque, char *error) {
    DnDecoder *d = av_mallocz(sizeof(*d));
    int result = AVERROR(ENOMEM);
    AVDictionary *options = NULL;
    if (!d) { dn_error(result, error); return NULL; }
    d->audio = audio; d->start = start; d->end = end;
    d->interrupt = interrupt; d->opaque = opaque;
    d->filter = av_strdup(filter);
    d->format = avformat_alloc_context();
    d->packet = av_packet_alloc(); d->decoded = av_frame_alloc(); d->filtered = av_frame_alloc();
    if (!d->format || !d->packet || !d->decoded || !d->filtered || !d->filter) goto fail;
    d->format->interrupt_callback = (AVIOInterruptCB){interrupted, d};
    av_dict_set(&options, "protocol_whitelist", "file", 0);
    result = avformat_open_input(&d->format, path, NULL, &options);
    av_dict_free(&options);
    if (result < 0 || (result = avformat_find_stream_info(d->format, NULL)) < 0) goto fail;
    d->stream = audio ? av_find_best_stream(d->format, AVMEDIA_TYPE_AUDIO, -1, -1, NULL, 0) : dn_video_stream(d->format);
    if (d->stream < 0) { result = AVERROR_STREAM_NOT_FOUND; goto fail; }
    AVStream *stream = d->format->streams[d->stream];
    const AVCodec *codec = avcodec_find_decoder(stream->codecpar->codec_id);
    if (!codec) { result = AVERROR_DECODER_NOT_FOUND; goto fail; }
    d->codec = avcodec_alloc_context3(codec);
    if (!d->codec) { result = AVERROR(ENOMEM); goto fail; }
    if ((result = avcodec_parameters_to_context(d->codec, stream->codecpar)) < 0) goto fail;
    d->codec->pkt_timebase = stream->time_base;
    d->codec->thread_count = 1;
    if ((result = avcodec_open2(d->codec, codec, NULL)) < 0) goto fail;
    double origin = d->format->start_time == AV_NOPTS_VALUE ? 0 : d->format->start_time / (double)AV_TIME_BASE;
    d->start += origin; d->end += origin;
    if (start > 0) {
        int64_t target = (int64_t)(d->start * AV_TIME_BASE);
        result = avformat_seek_file(d->format, -1, INT64_MIN, target, target, 0);
        if (result < 0) goto fail;
        avcodec_flush_buffers(d->codec);
    }
    return d;
fail:
    av_dict_free(&options);
    dn_error(result, error);
    dn_decode_free(d);
    return NULL;
}

/* 用第一张实际解码帧建立独立单线程滤镜图，先精确裁剪再归零时间。
   视频自动旋转后应用宿主缩放/fps，音频统一 48kHz 交错 float 立体声；失败由调用者回收图。 */
static int filters(DnDecoder *d, AVFrame *frame) {
    char args[1024], description[2048], rotation[64] = "";
    AVStream *stream = d->format->streams[d->stream];
    AVRational tb = stream->time_base;
    d->graph = avfilter_graph_alloc();
    AVFilterInOut *inputs = avfilter_inout_alloc(), *outputs = avfilter_inout_alloc();
    int result = AVERROR(ENOMEM);
    if (!d->graph || !inputs || !outputs) goto done;
    d->graph->nb_threads = 1;
    if (d->audio) {
        char layout[256];
        if (!frame->ch_layout.nb_channels) av_channel_layout_default(&frame->ch_layout, d->codec->ch_layout.nb_channels);
        av_channel_layout_describe(&frame->ch_layout, layout, sizeof(layout));
        snprintf(args, sizeof(args), "time_base=%d/%d:sample_rate=%d:sample_fmt=%s:channel_layout=%s",
            tb.num, tb.den, frame->sample_rate, av_get_sample_fmt_name(frame->format), layout);
        snprintf(description, sizeof(description), "atrim=start=%.9f:end=%.9f,asetpts=PTS-%.9f/TB,aresample=48000,aformat=sample_fmts=flt:channel_layouts=stereo",
            d->start, d->end, d->start);
    } else {
        AVRational sar = frame->sample_aspect_ratio;
        if (sar.num <= 0 || sar.den <= 0) sar = av_guess_sample_aspect_ratio(d->format, stream, frame);
        if (sar.num <= 0 || sar.den <= 0) sar = (AVRational){1, 1};
        snprintf(args, sizeof(args), "video_size=%dx%d:pix_fmt=%d:time_base=%d/%d:pixel_aspect=%d/%d:colorspace=%d:range=%d",
            frame->width, frame->height, frame->format, tb.num, tb.den, sar.num, sar.den, frame->colorspace, frame->color_range);
        double angle = fmod(dn_rotation(stream) + 360, 360);
        if (fabs(angle - 90) < 1) strcpy(rotation, "transpose=clock,");
        else if (fabs(angle - 270) < 1) strcpy(rotation, "transpose=cclock,");
        else if (fabs(angle - 180) < 1) strcpy(rotation, "hflip,vflip,");
        snprintf(description, sizeof(description), "trim=start=%.9f:end=%.9f,setpts=PTS-%.9f/TB,%s%s",
            d->start, d->end, d->start, rotation, d->filter);
    }
    result = avfilter_graph_create_filter(&d->source, avfilter_get_by_name(d->audio ? "abuffer" : "buffer"), "in", args, NULL, d->graph);
    if (result < 0) goto done;
    result = avfilter_graph_create_filter(&d->sink, avfilter_get_by_name(d->audio ? "abuffersink" : "buffersink"), "out", NULL, NULL, d->graph);
    if (result < 0) goto done;
    outputs->name = av_strdup("in"); outputs->filter_ctx = d->source; outputs->pad_idx = 0;
    inputs->name = av_strdup("out"); inputs->filter_ctx = d->sink; inputs->pad_idx = 0;
    if (!outputs->name || !inputs->name) { result = AVERROR(ENOMEM); goto done; }
    result = avfilter_graph_parse_ptr(d->graph, description, &inputs, &outputs, NULL);
    if (result >= 0) result = avfilter_graph_config(d->graph, NULL);
done:
    avfilter_inout_free(&inputs); avfilter_inout_free(&outputs);
    return result;
}

/* 从 send/receive 状态机取得下一滤镜帧；处理 EAGAIN、尾包排空和可恢复损坏包。
   正常 EOF 返回零，成功一帧返回一；完全损坏且未产生帧时仍报告 Invalid data。 */
static int next_frame(DnDecoder *d) {
    for (;;) {
        if (interrupted(d)) return AVERROR_EXIT;
        int result;
        if (d->graph) {
            result = av_buffersink_get_frame(d->sink, d->filtered);
            if (result >= 0) return 1;
            if (result == AVERROR_EOF) return d->invalid && !d->samples ? AVERROR_INVALIDDATA : 0;
            if (result != AVERROR(EAGAIN)) return result;
        }
        if (d->codec_eof) return d->invalid && !d->samples ? AVERROR_INVALIDDATA : 0;
        result = avcodec_receive_frame(d->codec, d->decoded);
        if (result >= 0) {
            d->decoded->pts = d->decoded->best_effort_timestamp;
            if (d->decoded->pts == AV_NOPTS_VALUE) d->decoded->pts = 0;
            double timestamp = d->decoded->pts * av_q2d(d->format->streams[d->stream]->time_base);
            if (timestamp >= d->end) {
                d->codec_eof = 1;
                av_frame_unref(d->decoded);
                if (d->graph) {
                    result = av_buffersrc_add_frame_flags(d->source, NULL, 0);
                    if (result < 0) return result;
                }
                continue;
            }
            if (!d->graph && (result = filters(d, d->decoded)) < 0) return result;
            result = av_buffersrc_add_frame_flags(d->source, d->decoded, AV_BUFFERSRC_FLAG_KEEP_REF);
            av_frame_unref(d->decoded);
            if (result < 0) return result;
            continue;
        }
        if (result == AVERROR_EOF) {
            d->codec_eof = 1;
            if (d->graph && (result = av_buffersrc_add_frame_flags(d->source, NULL, 0)) < 0) return result;
            continue;
        }
        if (result != AVERROR(EAGAIN)) {
            if (result != AVERROR_INVALIDDATA) return result;
            d->invalid = 1;
        }
        if (d->pending_packet) {
            result = avcodec_send_packet(d->codec, d->packet);
            if (result == AVERROR(EAGAIN)) continue;
            av_packet_unref(d->packet); d->pending_packet = 0;
            if (result == AVERROR_INVALIDDATA) { d->invalid = 1; continue; }
            if (result < 0) return result;
            continue;
        }
        if (d->input_eof) {
            result = avcodec_send_packet(d->codec, NULL);
            if (result < 0 && result != AVERROR_EOF && result != AVERROR(EAGAIN)) return result;
            continue;
        }
        result = av_read_frame(d->format, d->packet);
        if (result == AVERROR_EOF) { d->input_eof = 1; continue; }
        if (result < 0) return result;
        if (d->packet->stream_index == d->stream) d->pending_packet = 1;
        else av_packet_unref(d->packet);
    }
}

/* 将下一 BGRA 帧或一块交错 PCM 复制到宿主缓冲；长度参数阻止跨边界写入。
   返回字节数、零表示 EOF、负值表示实际错误；音频余量保存在当前帧，内存有界。 */
int dn_decode_read(DnDecoder *d, uint8_t *bytes, int capacity, char *error) {
    if (!d || !bytes || capacity <= 0) return dn_error(AVERROR(EINVAL), error);
    if (d->audio && d->position < d->filtered->nb_samples * 8) {
        int count = FFMIN(capacity, d->filtered->nb_samples * 8 - d->position);
        memcpy(bytes, d->filtered->data[0] + d->position, count); d->position += count;
        return count;
    }
    av_frame_unref(d->filtered);
    int result = next_frame(d);
    if (result <= 0) return dn_error(result, error);
    d->samples++;
    if (d->audio) {
        if (d->filtered->format != AV_SAMPLE_FMT_FLT || d->filtered->ch_layout.nb_channels != 2)
            return dn_error(AVERROR(EINVAL), error);
        d->position = FFMIN(capacity, d->filtered->nb_samples * 8);
        memcpy(bytes, d->filtered->data[0], d->position);
        return d->position;
    }
    if (d->filtered->format != AV_PIX_FMT_BGRA) return dn_error(AVERROR(EINVAL), error);
    result = av_image_copy_to_buffer(bytes, capacity, (const uint8_t *const *)d->filtered->data,
        d->filtered->linesize, AV_PIX_FMT_BGRA, d->filtered->width, d->filtered->height, 1);
    return dn_error(result, error);
}
