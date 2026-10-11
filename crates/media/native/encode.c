#include "media.h"
#include <math.h>
#include <stdio.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
#endif

struct DnEncoder {
    AVFormatContext *format;
    AVCodecContext *video, *audio;
    AVStream *vs, *as;
    AVFrame *vf, *af;
    AVPacket *packet;
    struct SwsContext *scale;
    FILE *pcm;
    int64_t video_pts, audio_pts, audio_end;
    int finished;
    DnInterrupt interrupt;
    void *opaque;
};

/* 以 UTF-8 路径打开只读 PCM；Windows 转为宽字符，避免中文路径随系统代码页变化。 */
static FILE *open_pcm(const char *path) {
#ifdef _WIN32
    int count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    if (!count) return NULL;
    wchar_t *wide = av_malloc_array(count, sizeof(wchar_t));
    if (!wide) return NULL;
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, count);
    FILE *file = _wfopen(wide, L"rb");
    av_free(wide);
    return file;
#else
    return fopen(path, "rb");
#endif
}

/* 取消回调由宿主持有，输出 IO 的阻塞操作可查询；不改写全局 libav 状态。 */
static int interrupted(void *opaque) {
    DnEncoder *e = opaque;
    return e->interrupt && e->interrupt(e->opaque);
}

/* 释放编码会话所有资源，失败/取消也关闭文件；不删除宿主临时输出或用户源文件。 */
void dn_encode_free(DnEncoder *e) {
    if (!e) return;
    if (e->pcm) fclose(e->pcm);
    sws_freeContext(e->scale);
    av_frame_free(&e->vf); av_frame_free(&e->af); av_packet_free(&e->packet);
    avcodec_free_context(&e->video); avcodec_free_context(&e->audio);
    if (e->format) {
        if (e->format->pb) avio_closep(&e->format->pb);
        avformat_free_context(e->format);
    }
    av_free(e);
}

/* 按指定封装打开原生编码器，配置已批准的实现专用参数；audio_path 为空则仅编码视频。
   输出仅是宿主独占的临时文件；所有初始化错误释放句柄，未识别选项也拒绝继续。 */
DnEncoder *dn_encode_open(const char *path, const char *audio_path, const DnEncoding *config,
    DnInterrupt interrupt, void *opaque, char *error) {
    DnEncoder *e = av_mallocz(sizeof(*e));
    AVDictionary *options = NULL;
    int result = AVERROR(ENOMEM);
    if (!e) { dn_error(result, error); return NULL; }
    e->interrupt = interrupt; e->opaque = opaque;
    if (config->width < 16 || config->height < 16 || config->width > 8192 || config->height > 8192 ||
        config->width % 2 || config->height % 2 || config->fps < 1 || config->fps > 240 ||
        config->bitrate <= 0 || !isfinite(config->duration) || config->duration <= 0) {
        result = AVERROR(EINVAL); goto fail;
    }
    const AVCodec *video = avcodec_find_encoder_by_name(config->encoder);
    if (!video) { result = AVERROR_ENCODER_NOT_FOUND; goto fail; }
    result = avformat_alloc_output_context2(&e->format, NULL, config->container, path);
    if (result < 0 || !e->format) goto fail;
    e->format->interrupt_callback = (AVIOInterruptCB){interrupted, e};
    e->vs = avformat_new_stream(e->format, NULL);
    e->video = avcodec_alloc_context3(video);
    e->vf = av_frame_alloc(); e->packet = av_packet_alloc();
    if (!e->vs || !e->video || !e->vf || !e->packet) { result = AVERROR(ENOMEM); goto fail; }
    e->video->width = config->width; e->video->height = config->height;
    e->video->pix_fmt = av_get_pix_fmt(config->pixel_format);
    e->video->bit_rate = config->bitrate;
    e->video->time_base = (AVRational){1, config->fps};
    e->video->framerate = (AVRational){config->fps, 1};
    e->video->sample_aspect_ratio = (AVRational){1, 1};
    e->video->thread_count = 2;
    if (e->format->oformat->flags & AVFMT_GLOBALHEADER) e->video->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    result = av_dict_parse_string(&options, config->options, "=", ":", 0);
    if (result < 0) goto fail;
    result = avcodec_open2(e->video, video, &options);
    if (result >= 0 && av_dict_count(options)) result = AVERROR_OPTION_NOT_FOUND;
    av_dict_free(&options);
    if (result < 0) goto fail;
    e->vs->time_base = e->video->time_base;
    e->vs->avg_frame_rate = e->video->framerate;
    if ((result = avcodec_parameters_from_context(e->vs->codecpar, e->video)) < 0) goto fail;
    e->vf->format = e->video->pix_fmt; e->vf->width = config->width; e->vf->height = config->height;
    if ((result = av_frame_get_buffer(e->vf, 32)) < 0) goto fail;
    e->scale = sws_getContext(config->width, config->height, AV_PIX_FMT_BGRA,
        config->width, config->height, e->video->pix_fmt, SWS_BICUBIC, NULL, NULL, NULL);
    if (!e->scale) { result = AVERROR(ENOMEM); goto fail; }
    if (audio_path) {
        e->pcm = open_pcm(audio_path);
        if (!e->pcm) { result = AVERROR(errno ? errno : EIO); goto fail; }
        const AVCodec *aac = avcodec_find_encoder_by_name("aac");
        if (!aac) { result = AVERROR_ENCODER_NOT_FOUND; goto fail; }
        e->audio = avcodec_alloc_context3(aac); e->af = av_frame_alloc();
        e->as = avformat_new_stream(e->format, NULL);
        if (!e->audio || !e->af || !e->as) { result = AVERROR(ENOMEM); goto fail; }
        e->audio->sample_fmt = AV_SAMPLE_FMT_FLTP; e->audio->sample_rate = 48000;
        e->audio->time_base = (AVRational){1, 48000}; e->audio->bit_rate = 192000;
        av_channel_layout_default(&e->audio->ch_layout, 2);
        if (e->format->oformat->flags & AVFMT_GLOBALHEADER) e->audio->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
        if ((result = avcodec_open2(e->audio, aac, NULL)) < 0) goto fail;
        e->as->time_base = e->audio->time_base;
        if ((result = avcodec_parameters_from_context(e->as->codecpar, e->audio)) < 0) goto fail;
        e->af->format = e->audio->sample_fmt; e->af->sample_rate = 48000;
        e->af->nb_samples = e->audio->frame_size;
        if ((result = av_channel_layout_copy(&e->af->ch_layout, &e->audio->ch_layout)) < 0) goto fail;
        if ((result = av_frame_get_buffer(e->af, 0)) < 0) goto fail;
        e->audio_end = (int64_t)llround(config->duration * 48000);
    }
    if (!(e->format->oformat->flags & AVFMT_NOFILE)) {
        result = avio_open2(&e->format->pb, path, AVIO_FLAG_WRITE, &e->format->interrupt_callback, NULL);
        if (result < 0) goto fail;
    }
    if (!strcmp(config->container, "mp4")) av_dict_set(&options, "movflags", "+faststart", 0);
    result = avformat_write_header(e->format, &options);
    av_dict_free(&options);
    if (result < 0) goto fail;
    return e;
fail:
    av_dict_free(&options);
    dn_error(result, error); dn_encode_free(e);
    return NULL;
}

/* 发送一帧或排空标志并交错写出所有可用编码包；重标时间基，保留真实写盘/编码错误。 */
static int packets(DnEncoder *e, AVCodecContext *codec, AVStream *stream, AVFrame *frame) {
    if (interrupted(e)) return AVERROR_EXIT;
    int result = avcodec_send_frame(codec, frame);
    if (result < 0) return result;
    while ((result = avcodec_receive_packet(codec, e->packet)) >= 0) {
        if (interrupted(e)) { av_packet_unref(e->packet); return AVERROR_EXIT; }
        av_packet_rescale_ts(e->packet, codec->time_base, stream->time_base);
        e->packet->stream_index = stream->index;
        result = av_interleaved_write_frame(e->format, e->packet);
        av_packet_unref(e->packet);
        if (result < 0) return result;
    }
    return result == AVERROR(EAGAIN) || result == AVERROR_EOF ? 0 : result;
}

/* 将精确长度的交错 PCM 转为 AAC 的平面 float，按视频进度交错写入且保留最后短帧。
   不填补缺失文件内容；截断/读错立即失败，不发布缺音频的输出。 */
static int audio_until(DnEncoder *e, int64_t target) {
    if (!e->audio) return 0;
    target = FFMIN(target, e->audio_end);
    while (e->audio_pts < target) {
        if (interrupted(e)) return AVERROR_EXIT;
        int result = av_frame_make_writable(e->af);
        if (result < 0) return result;
        e->af->nb_samples = (int)FFMIN(e->audio->frame_size, e->audio_end - e->audio_pts);
        for (int i = 0; i < e->af->nb_samples; i++) {
            float stereo[2];
            if (fread(stereo, sizeof(float), 2, e->pcm) != 2) return AVERROR(EIO);
            ((float *)e->af->data[0])[i] = stereo[0];
            ((float *)e->af->data[1])[i] = stereo[1];
        }
        e->af->pts = e->audio_pts;
        e->audio_pts += e->af->nb_samples;
        if ((result = packets(e, e->audio, e->as, e->af)) < 0) return result;
    }
    return 0;
}

/* 编码一张完整 BGRA 帧；长度必须匹配设置，复用色彩转换缓冲并推进时间戳。
   出错或已完成时拒绝写入，取消返回 libav EXIT；输出尚未发布给用户。 */
int dn_encode_write(DnEncoder *e, const uint8_t *pixels, int length, char *error) {
    if (!e || e->finished || !pixels || length != e->video->width * e->video->height * 4)
        return dn_error(AVERROR(EINVAL), error);
    int result = av_frame_make_writable(e->vf);
    if (result >= 0) {
        const uint8_t *source[4] = {pixels, NULL, NULL, NULL};
        int stride[4] = {e->video->width * 4, 0, 0, 0};
        result = sws_scale(e->scale, source, stride, 0, e->video->height, e->vf->data, e->vf->linesize);
    }
    if (result >= 0) {
        e->vf->pts = e->video_pts++;
        result = packets(e, e->video, e->vs, e->vf);
    }
    if (result >= 0) result = audio_until(e, av_rescale_q(e->video_pts, e->video->time_base, (AVRational){1, 48000}));
    return dn_error(result, error);
}

/* 排空音视频编码器后写入封装尾部并关闭输出；任何错误均返回，不把部分输出算作成功。 */
int dn_encode_finish(DnEncoder *e, char *error) {
    if (!e || e->finished) return dn_error(AVERROR(EINVAL), error);
    int result = audio_until(e, e->audio_end);
    if (result >= 0) result = packets(e, e->video, e->vs, NULL);
    if (result >= 0 && e->audio) result = packets(e, e->audio, e->as, NULL);
    if (result >= 0) result = av_write_trailer(e->format);
    e->finished = 1;
    if (e->format->pb) {
        int closed = avio_closep(&e->format->pb);
        if (result >= 0) result = closed;
    }
    return dn_error(result, error);
}
