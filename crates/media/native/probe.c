#include "media.h"
#include <math.h>
#include <string.h>

/* 返回固定桥接 ABI；同时核实实际加载的 libav 主版本，版本不匹配时拒绝使用。 */
int dn_abi(void) {
    return (avcodec_version() >> 16) == 62 && (avformat_version() >> 16) == 62 &&
        (avfilter_version() >> 16) == 11 && (avutil_version() >> 16) == 60 ? 1 : 0;
}

/* 将 libav 负错误码转换为调用者独占的 512 字节诊断；不保存全局错误状态。 */
int dn_error(int result, char *error) {
    if (result < 0) av_strerror(result, error, DN_ERROR_SIZE);
    return result;
}

/* 返回第一个非封面视频流；封面图片不得把纯音频误分类为视频。 */
int dn_video_stream(AVFormatContext *format) {
    for (unsigned i = 0; i < format->nb_streams; i++)
        if (format->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO &&
            !(format->streams[i]->disposition & AV_DISPOSITION_ATTACHED_PIC)) return (int)i;
    return -1;
}

/* 借用流的显示矩阵；大小不足或不存在返回空，指针仅在所属输入上下文存活期间有效。 */
const int32_t *dn_display_matrix(const AVStream *stream) {
    const AVPacketSideData *side = av_packet_side_data_get(stream->codecpar->coded_side_data,
        stream->codecpar->nb_coded_side_data, AV_PKT_DATA_DISPLAYMATRIX);
    return side && side->size >= 9 * sizeof(int32_t) ? (const int32_t *)side->data : NULL;
}

/* 读取显示矩阵或旧 rotate 标签中的旋转角；按整数角归一矩阵误差，无元数据返回零。 */
double dn_rotation(const AVStream *stream) {
    const int32_t *matrix = dn_display_matrix(stream);
    if (matrix) {
        double value = -round(av_display_rotation_get(matrix));
        if (isfinite(value)) return value;
    }
    AVDictionaryEntry *tag = av_dict_get(stream->metadata, "rotate", NULL, 0);
    return tag ? strtod(tag->value, NULL) : 0;
}

/* 只读本地文件并探测流、时长和显示尺寸；失败关闭输入并返回 libav 诊断。
   音频封面排除，尺寸包含像素比例与旋转；不创建子进程、不写源文件。 */
int dn_probe(const char *path, DnInfo *info, char *error) {
    AVFormatContext *format = NULL;
    AVDictionary *options = NULL;
    memset(info, 0, sizeof(*info));
    av_dict_set(&options, "protocol_whitelist", "file", 0);
    int result = avformat_open_input(&format, path, NULL, &options);
    av_dict_free(&options);
    if (result >= 0) result = avformat_find_stream_info(format, NULL);
    if (result >= 0) {
        if (format->duration != AV_NOPTS_VALUE) info->duration = format->duration / (double)AV_TIME_BASE;
        int video = dn_video_stream(format);
        for (unsigned i = 0; i < format->nb_streams; i++) {
            AVStream *stream = format->streams[i];
            if (stream->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) info->audio = 1;
            if ((format->duration == AV_NOPTS_VALUE || format->duration <= 0) && stream->duration != AV_NOPTS_VALUE)
                info->duration = fmax(info->duration, stream->duration * av_q2d(stream->time_base));
        }
        if (video >= 0) {
            AVStream *stream = format->streams[video];
            AVRational sar = av_guess_sample_aspect_ratio(format, stream, NULL);
            double width = stream->codecpar->width * (sar.num > 0 && sar.den > 0 ? av_q2d(sar) : 1);
            double height = stream->codecpar->height;
            if (fmod(fabs(dn_rotation(stream)), 180) == 90) {
                double swap = width; width = height; height = swap;
            }
            if (!isfinite(width) || width <= 0 || height <= 0 || width * height > 67108864)
                result = AVERROR(EINVAL);
            else {
                info->video = 1; info->width = (int)round(width); info->height = (int)round(height);
            }
        }
    }
    avformat_close_input(&format);
    return dn_error(result, error);
}
