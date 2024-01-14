#ifndef TRANSCODER_H_
#define TRANSCODER_H_

void ffmpeg_transcode(const char *input_file_name, const char *output_file_name, int target_width, int target_height, int debug);

#endif // TRANSCODER_H_
