Feature: Transcoding video files

  Scenario: When the server gets a request with a video in it, it responds with a downscaled mp4 and a thumbnail
    Given VideoService is running
    When a TranscodeRequest with samples/SampleVideo_1280x720_1mb.mp4 is received
    Then a downscaled mp4 is returned
