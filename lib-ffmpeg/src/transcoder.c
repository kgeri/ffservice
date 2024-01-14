// Based on https://www.ffmpeg.org/doxygen/trunk/transcode_8c_source.html, this module is responsible for transcoding videos

#include <libavcodec/avcodec.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/opt.h>
#include "transcoder.h"

typedef struct StreamContext
{
    AVCodecContext *dec_ctx;
    AVCodecContext *enc_ctx;
    AVFrame *dec_frame;
} StreamContext;

static void sc_free(StreamContext *sc)
{
    avcodec_free_context(&sc->dec_ctx);
    avcodec_free_context(&sc->enc_ctx);
    av_frame_free(&sc->dec_frame);
}

typedef struct FilterContext
{
    AVFilterContext *buffersrc_ctx;
    AVFilterContext *buffersink_ctx;
    AVFilterGraph *filter_graph;
    AVPacket *enc_pkt;
    AVFrame *filtered_frame;
} FilterContext;

static void fc_free(FilterContext *fc)
{
    avfilter_graph_free(&fc->filter_graph);
    av_packet_free(&fc->enc_pkt);
    av_frame_free(&fc->filtered_frame);
}

typedef struct TranscodeContext
{
    AVFormatContext *ifmt_ctx;
    AVFormatContext *ofmt_ctx;
    StreamContext *stream_ctxs;
    unsigned int nb_stream_ctxs;
    FilterContext *filter_ctxs;
    unsigned int nb_filter_ctxs;
    AVIOContext *output_pb;
} TranscodeContext;

static void tc_free(TranscodeContext *tc)
{
    for (unsigned int i = 0; i < tc->nb_stream_ctxs; i++)
        sc_free(&tc->stream_ctxs[i]);
    av_freep(&tc->stream_ctxs);
    tc->nb_stream_ctxs = 0;

    for (unsigned int i = 0; i < tc->nb_filter_ctxs; i++)
        fc_free(&tc->filter_ctxs[i]);
    av_freep(&tc->filter_ctxs);
    tc->nb_filter_ctxs = 0;

    avformat_close_input(&tc->ifmt_ctx);
    if (tc->output_pb)
        avio_closep(&tc->output_pb);
    avformat_free_context(tc->ofmt_ctx);
}

/**
 * Opens an input file and configures the decoder part of the transcoding context.
 *
 * @param input_file_name name of the file to open.
 * @param tc  An empty TranscodeContext that will be initialized. Must be freed with `tc_free` after use.
 *
 * @return 0 on success, a negative AVERROR on failure.
 */
static int open_input_file(const char *input_file_name, TranscodeContext *tc)
{
    int ret;

    // Opening file and allocating input context
    AVFormatContext *ifmt_ctx = NULL;
    if ((ret = avformat_open_input(&ifmt_ctx, input_file_name, NULL, NULL)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Cannot open input file: %s\n", input_file_name);
        return ret;
    }
    tc->ifmt_ctx = ifmt_ctx;

    // Populating stream info
    if ((ret = avformat_find_stream_info(ifmt_ctx, NULL)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Cannot find stream information: %s\n", input_file_name);
        return ret;
    }

    // Allocating stream context array
    unsigned int nb_stream_ctxs = ifmt_ctx->nb_streams;
    StreamContext *stream_ctxs;
    stream_ctxs = av_calloc(nb_stream_ctxs, sizeof(*stream_ctxs));
    if (!stream_ctxs)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to stream contexts\n");
        return AVERROR(ENOMEM);
    }
    tc->stream_ctxs = stream_ctxs;
    tc->nb_stream_ctxs = nb_stream_ctxs;

    // Configuring StreamContexts
    for (unsigned int i = 0; i < nb_stream_ctxs; i++)
    {
        // Locating decoder
        AVStream *stream = ifmt_ctx->streams[i];
        const AVCodec *dec = avcodec_find_decoder(stream->codecpar->codec_id);
        if (!dec)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to find decoder for stream#%u\n", i);
            return AVERROR_DECODER_NOT_FOUND;
        }

        // Allocating codec context for the decoder, assigning it to the StreamContext
        AVCodecContext *codec_ctx = avcodec_alloc_context3(dec);
        if (!codec_ctx)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to allocate the decoder context for stream #%u\n", i);
            return AVERROR(ENOMEM);
        }
        stream_ctxs[i].dec_ctx = codec_ctx;

        if ((ret = avcodec_parameters_to_context(codec_ctx, stream->codecpar)) < 0)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to copy decoder parameters for stream #%u\n", i);
            return ret;
        }

        codec_ctx->pkt_timebase = stream->time_base;
        if (codec_ctx->codec_type == AVMEDIA_TYPE_VIDEO || codec_ctx->codec_type == AVMEDIA_TYPE_AUDIO)
        {
            if (codec_ctx->codec_type == AVMEDIA_TYPE_VIDEO)
            {
                codec_ctx->framerate = av_guess_frame_rate(ifmt_ctx, stream, NULL);
            }
            if ((ret = avcodec_open2(codec_ctx, dec, NULL)))
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to open decoder for stream #%u\n", i);
                return ret;
            }
        }

        // Allocating a frame buffer
        stream_ctxs[i].dec_frame = av_frame_alloc();
        if (!stream_ctxs[i].dec_frame)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to allocate frame for stream #%u\n", i);
            return AVERROR(ENOMEM);
        }
    }

    return 0;
}

/**
 * Opens an input file and configures the encoder part of the transcoding context.
 *
 * @param output_file_name name of the file to transcode to.
 * @param tc  A TranscodeContext already initialized by `open_input_file`. Must be freed with `tc_free` after use.
 *
 * @return 0 on success, a negative AVERROR on failure.
 */
static int open_output_file(const char *output_file_name, TranscodeContext *tc)
{
    int ret;
    AVFormatContext *ofmt_ctx = NULL;
    avformat_alloc_output_context2(&ofmt_ctx, NULL, NULL, output_file_name);
    if (!ofmt_ctx)
    {
        av_log(NULL, AV_LOG_ERROR, "Could not create output context\n");
        return AVERROR_UNKNOWN;
    }
    tc->ofmt_ctx = ofmt_ctx;

    // TODO externalize?
    const AVCodec *audio_encoder = avcodec_find_encoder(AV_CODEC_ID_AAC);
    if (!audio_encoder)
    {
        av_log(NULL, AV_LOG_FATAL, "Audio encoder not found (AAC)\n");
        return AVERROR_INVALIDDATA;
    }

    // TODO externalize?
    const AVCodec *video_encoder = avcodec_find_encoder(AV_CODEC_ID_H264);
    if (!video_encoder)
    {
        av_log(NULL, AV_LOG_FATAL, "Video encoder not found (H264)\n");
        return AVERROR_INVALIDDATA;
    }

    for (unsigned int i = 0; i < tc->ifmt_ctx->nb_streams; i++)
    {
        AVStream *out_stream = avformat_new_stream(ofmt_ctx, NULL);
        if (!out_stream)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to allocating output stream #%u\n", i);
            return AVERROR_UNKNOWN;
        }

        AVStream *in_stream = tc->ifmt_ctx->streams[i];
        AVCodecContext *dec_ctx = tc->stream_ctxs[i].dec_ctx;
        enum AVMediaType codec_type = dec_ctx->codec_type;

        if (codec_type == AVMEDIA_TYPE_AUDIO || codec_type == AVMEDIA_TYPE_VIDEO)
        {
            const AVCodec *encoder = codec_type == AVMEDIA_TYPE_VIDEO ? video_encoder : audio_encoder;
            AVCodecContext *enc_ctx = avcodec_alloc_context3(encoder);
            if (!enc_ctx)
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to allocate encoder context for stream #%u\n", i);
                return AVERROR(ENOMEM);
            }
            tc->stream_ctxs[i].enc_ctx = enc_ctx;

            if (codec_type == AVMEDIA_TYPE_VIDEO)
            {
                enc_ctx->width = dec_ctx->width;
                enc_ctx->height = dec_ctx->height;
                enc_ctx->sample_aspect_ratio = dec_ctx->sample_aspect_ratio;
                enc_ctx->pix_fmt = encoder->pix_fmts[0];           // Take first format from list of supported formats
                enc_ctx->time_base = av_inv_q(dec_ctx->framerate); // Video time_base can be set to whatever is handy and supported by encoder
            }
            else if (codec_type == AVMEDIA_TYPE_AUDIO)
            {
                enc_ctx->sample_rate = dec_ctx->sample_rate;
                if ((ret = av_channel_layout_copy(&enc_ctx->ch_layout, &dec_ctx->ch_layout)) < 0)
                {
                    av_log(NULL, AV_LOG_ERROR, "Failed to copy AV channel layour for stream #%u\n", i);
                    return ret;
                }
                enc_ctx->sample_fmt = encoder->sample_fmts[0]; // Take first format from list of supported formats
                enc_ctx->time_base = (AVRational){1, enc_ctx->sample_rate};
            }

            // Common setup
            if (ofmt_ctx->oformat->flags & AVFMT_GLOBALHEADER)
                enc_ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;

            if ((ret = avcodec_open2(enc_ctx, encoder, NULL)) < 0) // TODO pass options to H264 in third param?
            {
                av_log(NULL, AV_LOG_ERROR, "Cannot open %s encoder for stream #%u\n", encoder->name, i);
                return ret;
            }

            if ((ret = avcodec_parameters_from_context(out_stream->codecpar, enc_ctx)) < 0)
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to copy encoder parameters to output stream #%u\n", i);
                return ret;
            }

            out_stream->time_base = enc_ctx->time_base;
        }
        else if (codec_type == AVMEDIA_TYPE_UNKNOWN)
        {
            av_log(NULL, AV_LOG_FATAL, "Stream #%u is of unknown type, cannot proceed\n", i);
            return AVERROR_INVALIDDATA;
        }
        else // Muxing only (stream copy)
        {
            if ((ret = avcodec_parameters_copy(out_stream->codecpar, in_stream->codecpar)) < 0)
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to copy parameters for stream #%u\n", i);
                return ret;
            }
            out_stream->time_base = in_stream->time_base;
        }
    }

    if (!(ofmt_ctx->oformat->flags & AVFMT_NOFILE))
    {
        if ((ret = avio_open(&ofmt_ctx->pb, output_file_name, AVIO_FLAG_WRITE)) < 0)
        {
            av_log(NULL, AV_LOG_ERROR, "Could not open output file: %s\n", output_file_name);
            return ret;
        }
        tc->output_pb = ofmt_ctx->pb;
    }

    // Init muxer, write output file header
    if ((ret = avformat_write_header(ofmt_ctx, NULL)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to write output file: %s\n", output_file_name);
        return ret;
    }

    return 0;
}

/**
 * Allocates and configures a video filter chain.
 *
 * @param filter the filter to configure. Must be freed with `fc_free` after use.
 * @param stream a StreamContext already initialized by `open_input_file`.
 *
 * @return 0 on success, a negative AVERROR on failure.
 */
static int init_video_filter(FilterContext *filter, StreamContext *stream, const char *filter_spec)
{
    AVCodecContext *dc = stream->dec_ctx;
    AVCodecContext *ec = stream->enc_ctx;
    char args[512];
    int ret = 0;
    const AVFilter *buffersrc = NULL;
    const AVFilter *buffersink = NULL;
    AVFilterContext *buffersrc_ctx = NULL;
    AVFilterContext *buffersink_ctx = NULL;
    AVFilterInOut *outputs = avfilter_inout_alloc();
    AVFilterInOut *inputs = avfilter_inout_alloc();
    AVFilterGraph *filter_graph = avfilter_graph_alloc();

    if (!outputs || !inputs || !filter_graph)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to allocate filter inputs/outputs/filter graph\n");
        ret = AVERROR(ENOMEM);
        goto end;
    }
    filter->filter_graph = filter_graph;

    buffersrc = avfilter_get_by_name("buffer");
    buffersink = avfilter_get_by_name("buffersink");
    if (!buffersrc || !buffersink)
    {
        av_log(NULL, AV_LOG_ERROR, "Filtering source or sink element not found\n");
        ret = AVERROR_UNKNOWN;
        goto end;
    }

    // TODO resize here?
    snprintf(args, sizeof(args), "video_size=%dx%d:pix_fmt=%d:time_base=%d/%d:pixel_aspect=%d/%d",
             dc->width, dc->height, dc->pix_fmt,
             dc->pkt_timebase.num, dc->pkt_timebase.den,
             dc->sample_aspect_ratio.num, dc->sample_aspect_ratio.den);

    if ((ret = avfilter_graph_create_filter(&buffersrc_ctx, buffersrc, "in", args, NULL, filter_graph)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Cannot create video buffer source\n");
        goto end;
    }
    filter->buffersrc_ctx = buffersrc_ctx;

    if ((ret = avfilter_graph_create_filter(&buffersink_ctx, buffersink, "out", NULL, NULL, filter_graph)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Cannot create video buffer sink\n");
        goto end;
    }
    filter->buffersink_ctx = buffersink_ctx;

    if ((ret = av_opt_set_bin(buffersink_ctx, "pix_fmts", (uint8_t *)&ec->pix_fmt, sizeof(ec->pix_fmt), AV_OPT_SEARCH_CHILDREN)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Cannot set output pixel format\n");
        goto end;
    }

    outputs->name = av_strdup("in");
    outputs->filter_ctx = buffersrc_ctx;
    outputs->pad_idx = 0;
    outputs->next = NULL;

    inputs->name = av_strdup("out");
    inputs->filter_ctx = buffersink_ctx;
    inputs->pad_idx = 0;
    inputs->next = NULL;

    if (!outputs->name || !inputs->name)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to allocate inputs/outputs name\n");
        ret = AVERROR(ENOMEM);
        goto end;
    }

    if ((ret = avfilter_graph_parse_ptr(filter_graph, filter_spec, &inputs, &outputs, NULL)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to parse filter graph\n");
        goto end;
    }

    if ((ret = avfilter_graph_config(filter_graph, NULL)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to configure filter graph\n");
        goto end;
    }

end:
    avfilter_inout_free(&inputs);
    avfilter_inout_free(&outputs);
    return ret;
}

/**
 * Allocates and configures filters for the TranscodingContext.
 *
 * @param tc  A TranscodeContext already initialized by `open_input_file`.
 *
 * @return 0 on success, a negative AVERROR on failure.
 */
static int init_filters(TranscodeContext *tc)
{
    int ret;

    unsigned int nb_filter_ctxs = tc->nb_stream_ctxs;
    FilterContext *filter_ctxs;
    filter_ctxs = av_calloc(nb_filter_ctxs, sizeof(*filter_ctxs));
    if (!filter_ctxs)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to allocate filters\n");
        return AVERROR(ENOMEM);
    }
    tc->filter_ctxs = filter_ctxs;
    tc->nb_filter_ctxs = nb_filter_ctxs;

    for (unsigned int i = 0; i < nb_filter_ctxs; i++)
    {
        StreamContext *sc = &tc->stream_ctxs[i];
        FilterContext *fc = &filter_ctxs[i];
        fc->buffersrc_ctx = NULL;
        fc->buffersink_ctx = NULL;
        fc->filter_graph = NULL;

        enum AVMediaType codec_type = sc->dec_ctx->codec_type;
        if (codec_type == AVMEDIA_TYPE_VIDEO)
        {
            if ((ret = init_video_filter(fc, sc, "null")))
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to init video filter (%d)\n", ret);
                return ret;
            }
        }
        else
        {
            continue;
        }

        fc->enc_pkt = av_packet_alloc();
        if (!fc->enc_pkt)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to allocate packet for filter (%d)\n", ret);
            return AVERROR(ENOMEM);
        }

        fc->filtered_frame = av_frame_alloc();
        if (!fc->filtered_frame)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to allocate frame for filter (%d)\n", ret);
            return AVERROR(ENOMEM);
        }
    }

    return 0;
}

/**
 * Encodes and writes a frame.
 *
 * @param tc  A fully initialized TranscodeContext.
 * @param stream_index  The stream index to write the packet to.
 * @param frame  The decoded frame, or NULL if this is the final flush operation.
 *
 * @return 0 on success, a negative AVERROR on failure.
 */
static int encode_write_frame(TranscodeContext *tc, unsigned int stream_index, AVFrame *frame)
{
    int ret;
    AVCodecContext *enc_ctx = tc->stream_ctxs[stream_index].enc_ctx;
    AVPacket *enc_pkt = tc->filter_ctxs[stream_index].enc_pkt;

    av_packet_unref(enc_pkt); // TODO this line was in the sample, but it doesn't seem to do anything (checked with -fsanitize=address)

    if (frame && frame->pts != AV_NOPTS_VALUE)
        frame->pts = av_rescale_q(frame->pts, frame->time_base, enc_ctx->time_base);

    // Send the frame to the encoder. The encoder may be buffering, so we need to read encoded packets in a loop.
    if ((ret = avcodec_send_frame(enc_ctx, frame)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to send frame to encoder\n");
        return ret;
    }

    while (ret >= 0)
    {
        // Read the next packet from the encoder. Packets are reference-counted, but this method always calls av_packet_unref as a first step.
        if ((ret = avcodec_receive_packet(enc_ctx, enc_pkt)) < 0)
        {
            if (ret == AVERROR_EOF || ret == AVERROR(EAGAIN))
                return 0;
            else
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to receive packet to encoder\n");
                return ret;
            }
        }

        // Prepare packet for muxing
        enc_pkt->stream_index = stream_index;
        av_packet_rescale_ts(enc_pkt,
                             enc_ctx->time_base,
                             tc->ofmt_ctx->streams[stream_index]->time_base);

        // Mux encoded frame and write it to the output file
        if ((ret = av_interleaved_write_frame(tc->ofmt_ctx, enc_pkt)) < 0)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to write encoded frame\n");
            return ret;
        }
    }

    return ret;
}

/**
 * Filters, encodes and writes a frame by:
 * - sending it to the buffered filter context
 * - receiving decoded frames into `FilterContext->filtered_frame`
 * - pushing the frames to the encoder
 *
 * @param tc  A fully initialized TranscodeContext.
 * @param stream_index  The stream index to write the packet to.
 * @param frame  The decoded frame, or NULL if this is the final flush operation.
 *
 * @return 0 on success, a negative AVERROR on failure.
 */
static int filter_encode_write_frame(TranscodeContext *tc, unsigned int stream_index, AVFrame *frame)
{
    int ret;
    FilterContext *fc = &tc->filter_ctxs[stream_index];

    // Push the decoded frame into the filtergraph. Note that the filter graph might return multiple
    // resulting frames, hence we need to read them in a loop next.
    if ((ret = av_buffersrc_add_frame_flags(fc->buffersrc_ctx, frame, 0)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Error while feeding the filtergraph\n");
        return ret;
    }

    while (ret >= 0)
    {
        // Read a frame from the sink of the filter graph into `FilterContext->filtered_frame`.
        if ((ret = av_buffersink_get_frame(fc->buffersink_ctx, fc->filtered_frame)) < 0)
        {
            // If no more frames for output - returns AVERROR(EAGAIN)
            // If flushed and no more frames for output - returns AVERROR_EOF
            // Rewrite retcode to 0 to show it as normal procedure completion
            if (ret == AVERROR_EOF || ret == AVERROR(EAGAIN))
            {
                return 0;
            }
        }

        fc->filtered_frame->time_base = av_buffersink_get_time_base(fc->buffersink_ctx);
        fc->filtered_frame->pict_type = AV_PICTURE_TYPE_NONE;

        // Push the filtered frame through the encoder, and write it to the output
        ret = encode_write_frame(tc, stream_index, fc->filtered_frame);
        av_frame_unref(fc->filtered_frame); // AVFrame is reference-counted, so we must release it after use
    }

    return ret;
}

/**
 * Transcodes an AVPacket by:
 * - sending it to the decoder
 * - receiving decoded frames into `StreamContext->dec_frame`
 * - pushing the frames through the filter graph and the encoder
 *
 * @param tc  A fully initialized TranscodeContext.
 * @param stream_index  The stream index to write the packet to.
 * @param packet  The packet read from the input, or NULL if this is the final flush operation.
 *
 * @return 0 on success, a negative AVERROR on failure.
 */
static int transcode_packet(TranscodeContext *tc, unsigned int stream_index, AVPacket *packet)
{
    int ret;
    StreamContext *sc = &tc->stream_ctxs[stream_index];
    int64_t pos = packet == NULL ? -1 : packet->pos;

    // Send packet to the decoder. One packet may contain multiple frames (typically for audio), so we need to read them in a loop.
    if ((ret = avcodec_send_packet(sc->dec_ctx, packet)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to decode packet #%ld (%d)\n", pos, ret);
        return ret;
    }

    while (ret >= 0)
    {
        // Read the next frame from the decoder, into StreamContext->dec_frame.
        // Note that AVFrames are reference-counted like AVPackets, but we don't really care because this method starts
        // by calling av_frame_unref on its input.
        if ((ret = avcodec_receive_frame(sc->dec_ctx, sc->dec_frame)) < 0)
        {
            if (ret == AVERROR_EOF || ret == AVERROR(EAGAIN))
                break;
            else
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to receive frame for packet #%ld (%d)\n", pos, ret);
                return ret;
            }
        }

        // Push the decoded frame through our filter graph
        sc->dec_frame->pts = sc->dec_frame->best_effort_timestamp; // TODO not sure what's this for?
        if ((ret = filter_encode_write_frame(tc, stream_index, sc->dec_frame)) < 0)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to encode packet #%ld (%d)\n", pos, ret);
            return ret;
        }
    }

    return 0;
}

void ffmpeg_transcode(const char *input_file_name, const char *output_file_name, int debug)
{
    int ret;
    TranscodeContext tc = {NULL, NULL, NULL, 0, NULL, 0, NULL};

    if (debug)
        av_log_set_level(AV_LOG_DEBUG);

    if ((ret = open_input_file(input_file_name, &tc)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to open input: %s (%d)\n", input_file_name, ret);
        goto end;
    }

    if ((ret = open_output_file(output_file_name, &tc)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to open output: %s (%d)\n", output_file_name, ret);
        goto end;
    }

    if (debug)
    {
        av_dump_format(tc.ifmt_ctx, 0, input_file_name, 0);
        av_dump_format(tc.ofmt_ctx, 0, output_file_name, 1);
    }

    // Filter setup
    if ((ret = init_filters(&tc)) < 0)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to configure filters (%d)\n", ret);
        goto end;
    }

    // Transcoding
    AVPacket *packet = av_packet_alloc();
    if (!packet)
    {
        av_log(NULL, AV_LOG_ERROR, "Failed to allocate packet\n");
        goto end;
    }

    for (unsigned int i = 0;; i++)
    {
        // Read the next _packet_ of the input. A packet may contain multiple frames, so this method name is somewhat misleading.
        // If this call is successful, packed will reference count, and contain the data for one or more frames.
        if ((ret = av_read_frame(tc.ifmt_ctx, packet)) < 0) // TODO this is also true if there was an error - how to detect that?
            break;

        unsigned int stream_index = packet->stream_index;
        FilterContext *fc = &tc.filter_ctxs[stream_index];

        if (fc->filter_graph)
        {
            // Transcode the packet. For each packet we read, we're pushing it through the filter chain in a number of steps.
            // See the docs and methods invoked by the below.
            if ((ret = transcode_packet(&tc, stream_index, packet)) < 0)
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to transcode packet #%ld (%d)\n", packet->pos, ret);
                goto end;
            }
        }
        else
        {
            // Remux without reencoding. This would do the same thing as we do above, but with a specialized method (av_interleaved_write_frame).
            av_packet_rescale_ts(packet,
                                 tc.ifmt_ctx->streams[stream_index]->time_base,
                                 tc.ofmt_ctx->streams[stream_index]->time_base); // TODO why do we need to do this exactly?
            if ((ret = av_interleaved_write_frame(tc.ofmt_ctx, packet)) < 0)
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to remux packet #%ld (%d)\n", packet->pos, ret);
                goto end;
            }
        }

        // AVPackets - unlike most things in ffmpeg - are reference-counted, so we should unref it, otherwise av_packet_free won't free it.
        av_packet_unref(packet);
    }

    // Flush decoders, filters and encoders
    for (unsigned int i = 0; i < tc.nb_stream_ctxs; i++)
    {
        if (!tc.filter_ctxs[i].filter_graph) // No need to flush remuxed streams
            continue;

        // Flush stream by processing a NULL packet
        if ((ret = transcode_packet(&tc, i, NULL)) < 0)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to flush stream #%u (%d)\n", i, ret);
            goto end;
        }

        // Flush filter by writing a NULL frame
        if ((ret = filter_encode_write_frame(&tc, i, NULL)) < 0)
        {
            av_log(NULL, AV_LOG_ERROR, "Failed to flush filter for stream #%u (%d)\n", i, ret);
            goto end;
        }

        // Flush encoder
        if (tc.stream_ctxs[i].enc_ctx->codec->capabilities & AV_CODEC_CAP_DELAY)
        {
            if ((ret = encode_write_frame(&tc, i, NULL)) < 0)
            {
                av_log(NULL, AV_LOG_ERROR, "Failed to flush encoder for stream #%u (%d)\n", i, ret);
                goto end;
            }
        }
    }

    // Write trailer
    av_write_trailer(tc.ofmt_ctx);
end:
    av_packet_free(&packet);
    tc_free(&tc);
}
