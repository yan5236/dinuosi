/* Dinuosi 与 FFmpeg 8 动态库之间的固定 C ABI；所有句柄由创建它的库释放。 */
#ifndef DINUOSI_MEDIA_H
#define DINUOSI_MEDIA_H
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <errno.h>
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersrc.h>
#include <libavfilter/buffersink.h>
#include <libavutil/opt.h>
#include <libavutil/display.h>
#include <libavutil/imgutils.h>
#include <libswscale/swscale.h>
#ifdef _WIN32
#define DN_API __declspec(dllexport)
#else
#define DN_API __attribute__((visibility("default")))
#endif
#define DN_ERROR_SIZE 512
typedef int (*DnInterrupt)(void *);
typedef struct {
    double duration;
    int video, audio, width, height;
} DnInfo;
typedef struct DnDecoder DnDecoder;
typedef struct DnEncoder DnEncoder;
typedef struct {
    int width, height, fps, bitrate;
    const char *encoder, *pixel_format, *options, *container;
    double duration;
} DnEncoding;
DN_API int dn_abi(void);
DN_API int dn_probe(const char *, DnInfo *, char *);
DN_API DnDecoder *dn_decode_open(const char *, int, double, double, const char *, DnInterrupt, void *, char *);
DN_API int dn_decode_read(DnDecoder *, uint8_t *, int, char *);
DN_API void dn_decode_free(DnDecoder *);
DN_API DnEncoder *dn_encode_open(const char *, const char *, const DnEncoding *, DnInterrupt, void *, char *);
DN_API int dn_encode_write(DnEncoder *, const uint8_t *, int, char *);
DN_API int dn_encode_finish(DnEncoder *, char *);
DN_API void dn_encode_free(DnEncoder *);
int dn_error(int, char *);
double dn_rotation(const AVStream *);
int dn_video_stream(AVFormatContext *);
#endif
