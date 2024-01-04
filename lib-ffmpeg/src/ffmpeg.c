#include <libavformat/avformat.h>
#include <stdio.h>
#include "ffmpeg.h"

void ffmpeg_open(const char *file_name)
{
    AVFormatContext *fmt_ctx = NULL;
    if (avformat_open_input(&fmt_ctx, file_name, NULL, NULL) < 0)
    {
        fprintf(stderr, "Could not open: %s\n", file_name);
        return;
    }

    AVCodec *dec = NULL;
    int stream_idx = av_find_best_stream(fmt_ctx, AVMEDIA_TYPE_VIDEO, -1, -1, &dec, 0);
    if (stream_idx < 0)
    {
        fprintf(stderr, "Could not find video stream in: %s\n", file_name);
        return;
    }

    av_dump_format(fmt_ctx, stream_idx, file_name, 0);
}