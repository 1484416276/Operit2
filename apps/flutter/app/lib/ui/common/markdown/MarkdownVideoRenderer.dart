// ignore_for_file: file_names

import 'package:flutter/material.dart';
import 'package:video_player/video_player.dart';

import '../../../core/logging/ClientLogger.dart';
import '../interactions/MessagePressShield.dart';
import 'MarkdownAudioRenderer.dart';
import 'MarkdownImageRenderer.dart';
import 'MarkdownLink.dart';

const Set<String> _markdownVideoExtensions = <String>{
  'mp4',
  'webm',
  'mkv',
  'mov',
  'm4v',
  '3gp',
  'avi',
  'ogv',
};

bool isLikelyVideoUrl(String url) {
  final extension = normalizeMarkdownMediaUrl(url).split('.').last;
  return _markdownVideoExtensions.contains(extension);
}

class MarkdownVideoRenderer extends StatefulWidget {
  const MarkdownVideoRenderer({
    super.key,
    required this.videoMarkdown,
    required this.textColor,
    this.maxVideoHeight = 220,
  });

  final String videoMarkdown;
  final Color textColor;
  final double maxVideoHeight;

  @override
  State<MarkdownVideoRenderer> createState() => _MarkdownVideoRendererState();
}

class _MarkdownVideoRendererState extends State<MarkdownVideoRenderer> {
  VideoPlayerController? _controller;
  Future<void>? _initializeFuture;
  bool _failed = false;

  @override
  void initState() {
    super.initState();
    _createController();
  }

  @override
  void didUpdateWidget(covariant MarkdownVideoRenderer oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.videoMarkdown != widget.videoMarkdown) {
      _releaseController();
      _createController();
    }
  }

  void _createController() {
    _failed = false;
    _initializeFuture = null;
    final videoUrl = extractMarkdownImageUrl(widget.videoMarkdown);
    final uri = Uri.tryParse(videoUrl);
    if (uri == null || (uri.scheme != 'http' && uri.scheme != 'https')) {
      _failed = true;
      return;
    }
    final controller = VideoPlayerController.networkUrl(uri);
    _controller = controller;
    controller.addListener(_handleControllerChanged);
    _initializeFuture = _initializeController(controller, videoUrl);
  }

  Future<void> _initializeController(
    VideoPlayerController controller,
    String videoUrl,
  ) async {
    try {
      await controller.initialize();
      if (mounted) {
        setState(() {});
      }
    } catch (error, stackTrace) {
      ClientLogger.w(
        'Cannot load markdown video: $videoUrl',
        tag: 'MarkdownVideo',
        error: error,
        stackTrace: stackTrace,
      );
      if (mounted) {
        setState(() {
          _failed = true;
        });
      }
    }
  }

  void _handleControllerChanged() {
    if (mounted) {
      setState(() {});
    }
  }

  void _releaseController() {
    final controller = _controller;
    _controller = null;
    _initializeFuture = null;
    if (controller == null) {
      return;
    }
    controller.removeListener(_handleControllerChanged);
    controller.dispose();
  }

  @override
  void dispose() {
    _releaseController();
    super.dispose();
  }

  Future<void> _togglePlayback() async {
    final controller = _controller;
    if (controller == null) {
      return;
    }
    try {
      if (controller.value.isPlaying) {
        await controller.pause();
      } else {
        await controller.play();
      }
    } catch (error, stackTrace) {
      ClientLogger.w(
        'Cannot play markdown video',
        tag: 'MarkdownVideo',
        error: error,
        stackTrace: stackTrace,
      );
      if (mounted) {
        setState(() {
          _failed = true;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    if (!isCompleteImageMarkdown(widget.videoMarkdown)) {
      return const SizedBox.shrink();
    }

    final videoAlt = extractMarkdownImageAlt(widget.videoMarkdown);
    final videoUrl = extractMarkdownImageUrl(widget.videoMarkdown);
    if (videoUrl.isEmpty || !isLikelyVideoUrl(videoUrl)) {
      return const SizedBox.shrink();
    }

    final theme = Theme.of(context);
    final initializeFuture = _initializeFuture;
    final controller = _controller;
    return Semantics(
      label: videoAlt.isNotEmpty ? 'Video: $videoAlt' : 'Video',
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: <Widget>[
            ClipRRect(
              borderRadius: BorderRadius.circular(12),
              child: ColoredBox(
                color: theme.colorScheme.surfaceContainerHighest.withValues(
                  alpha: 0.18,
                ),
                child: ConstrainedBox(
                  constraints: BoxConstraints(maxHeight: widget.maxVideoHeight),
                  child: _failed || controller == null || initializeFuture == null
                      ? _MarkdownMediaFailure(
                          label: videoAlt.isEmpty ? videoUrl : videoAlt,
                          onTap: () => activateMarkdownLink(videoUrl, null),
                        )
                      : FutureBuilder<void>(
                          future: initializeFuture,
                          builder: (context, snapshot) {
                            if (_failed || snapshot.hasError) {
                              return _MarkdownMediaFailure(
                                label: videoAlt.isEmpty ? videoUrl : videoAlt,
                                onTap: () => activateMarkdownLink(videoUrl, null),
                              );
                            }
                            final ready =
                                snapshot.connectionState ==
                                ConnectionState.done;
                            if (!ready) {
                              return const AspectRatio(
                                aspectRatio: 16 / 9,
                                child: Center(
                                  child: CircularProgressIndicator(),
                                ),
                              );
                            }
                            return Stack(
                              alignment: Alignment.center,
                              children: <Widget>[
                                AspectRatio(
                                  aspectRatio:
                                      controller.value.aspectRatio == 0
                                      ? 16 / 9
                                      : controller.value.aspectRatio,
                                  child: VideoPlayer(controller),
                                ),
                                MessagePressShieldRegion(
                                  child: IconButton.filledTonal(
                                    onPressed: _togglePlayback,
                                    icon: Icon(
                                      controller.value.isPlaying
                                          ? Icons.pause
                                          : Icons.play_arrow,
                                    ),
                                    tooltip: controller.value.isPlaying
                                        ? 'Pause'
                                        : 'Play',
                                  ),
                                ),
                              ],
                            );
                          },
                        ),
                ),
              ),
            ),
            if (videoAlt.isNotEmpty)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 2, vertical: 1),
                child: Text(
                  videoAlt,
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant.withValues(
                      alpha: 0.7,
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _MarkdownMediaFailure extends StatelessWidget {
  const _MarkdownMediaFailure({required this.label, required this.onTap});

  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final color = Theme.of(context).colorScheme.onSurfaceVariant;
    return MessagePressShieldRegion(
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 12, horizontal: 8),
          child: Row(
            children: <Widget>[
              Icon(Icons.broken_image_outlined, size: 18, color: color),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  label,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: color,
                    decoration: TextDecoration.underline,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
