# Audio level scales for Cue volume and microphone metering

Status: research note, not an architectural decision.

## Answer

There is no universal scientific meaning for **100% volume** that applies to
both playback and recording.

- In a playback UI, 100% normally means the application's highest control
  setting. It does not specify sound pressure at the listener, perceived
  loudness, or the device's maximum output.
- In a digital recording meter, the useful technical ceiling is digital full
  scale. `0 dBFS` is the maximum representable digital signal level, so a meter
  may map that ceiling to 100%. It does not mean ideal speech level or 100% of a
  microphone's physical capability.
- Professional audio uses decibels, true peak, and loudness measurements rather
  than percentage as interchangeable physical quantities.

## Playback

The WHATWG HTML standard defines media volume as a fraction from silent to the
element's loudest setting. It explicitly allows a non-linear range and says the
loudest setting may be below the system's loudest possible setting. Therefore,
even the web platform's own `1.0` is a relative control endpoint, not a calibrated
acoustic output. See [HTML Standard: media volume](https://html.spec.whatwg.org/multipage/media.html#dom-media-volume-dev).

For Web Audio, a `GainNode` multiplies every input sample by its gain. Its default
gain is `1`, which is unity gain rather than a definition of “100% loudness”. The
standard conversion is `dB = 20 log10(gain)`, so gain `2` is about `+6.02 dB` and
gain `4` is about `+12.04 dB`. See [Web Audio API: GainNode](https://www.w3.org/TR/webaudio/#gainnode) and
[Web Audio API: linear gain and decibels](https://www.w3.org/TR/webaudio/#computedValue-DynamicsCompressorNode).

Web Audio destination signals have a nominal range of `[-1, 1]`; rendition
outside it is unspecified. This is the relevant technical ceiling when adding
gain and overlapping cues, but it still says nothing about speaker sound
pressure or perceived loudness. See [Web Audio API: audio signal rendering](https://www.w3.org/TR/webaudio/#audio-signal-values-rendering).

Perceived loudness also depends on frequency and listening conditions. ISO 226
standardizes different sound-pressure/frequency combinations that listeners
perceive as equally loud; it does not define one application “100%”. See
[ISO 226:2023](https://www.iso.org/standard/83117.html).

### Cue-control calibration

The first implementation mapped 50% to gain `1` and 100% to gain `2`, so the
entire upper half added only about `6 dB`. Production sound synthesis caps a
single cue's sample peak near `0.08`, leaving substantial digital headroom. The
owner reported that the difference was not meaningful in the representative
listening environment.

The accepted calibration therefore uses perceptual speech anchors rather than
claiming an absolute acoustic percentage:

- keep 0% as mute and 50% as the former output;
- map 100% to about gain `8` (`+18 dB`) for loud-call headroom;
- use deliberate peak control for expected overlaps at the upper settings.

This is a product calibration, not compliance with a universal percentage
standard.

## Recording and microphone level

ITU-R defines `0 dBFS` as the maximum signal level a digital audio system can
represent. That is a useful definition of the top of a digital peak meter. See
[ITU-R BS.1726](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1726-0-200504-I%21%21PDF-E.pdf).

It is not an acoustic calibration. The mapping from sound in the room to dBFS
depends on microphone sensitivity, analogue preamplification, operating-system
input level, and signal processing. Browser microphone tracks can expose and
control automatic gain control, noise suppression, and echo cancellation when
the device and user agent support them. See [Media Capture and Streams: audio constrainable properties](https://www.w3.org/TR/mediacapture-streams/#def-constraint-autogaincontrol).

Professional practice keeps average loudness and peak headroom separate. ITU-R
BS.1770 defines algorithms for programme loudness and true peak; EBU R 128 uses
programme loudness for normalization and maximum true peak for the upper signal
limit. EBU's broadcast target is `-23 LUFS`, not 100%. See
[ITU-R BS.1770-5](https://www.itu.int/rec/R-REC-BS.1770-5-202311-I) and
[EBU R 128](https://tech.ebu.ch/docs/r/r128.pdf).

EBU Mode uses a 400 ms Momentary window, a 3 s Short-term window, and at least a
10 Hz update rate for live meters. A product confidence meter need not implement
EBU Mode, but this illustrates why ordinary level and peak are separate
measurements. See [EBU Tech 3341](https://tech.ebu.ch/docs/tech/tech3341.pdf).

### Microphone-meter calibration

The first implementation took the largest absolute sample in each analyser
window and mapped `-60 dBFS` to 0% and `0 dBFS` to 100%. A single transient near
full scale therefore drove both the main bar and its retained peak to 100%.
Normal speech readily reached 100% during review, showing that the main bar was
functioning as a peak meter rather than the intended practical speech confidence
indicator.

The accepted calibration separates the two signals:

- derive the moving bar from RMS or another short-window average level;
- derive the retained marker from sample peak;
- keep the logarithmic `-60 dBFS` to `0 dBFS` percentage mapping initially;
- reserve 100% for digital full scale, not “good” ordinary speech;
- continue to avoid quality labels because browser input is not acoustically
  calibrated.

If standards-grade loudness or clipping detection becomes a requirement later,
implement the complete BS.1770 loudness/true-peak algorithms rather than naming
a simple RMS/sample-peak meter as compliant.
