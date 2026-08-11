import 'dart:async';

import 'package:flutter/material.dart';
import 'package:video_player/video_player.dart';

class MediaTicketData {
  const MediaTicketData({required this.url, required this.expiresInSeconds});

  final String url;
  final int expiresInSeconds;
}

typedef TicketLoader = Future<MediaTicketData> Function();

/// Plays a short-lived Camrelay media ticket without exposing the API bearer
/// token to the media URL. The ticket loader is injected so the same surface
/// can later host a WebRTC adapter without changing navigation.
class TicketVideoScreen extends StatefulWidget {
  const TicketVideoScreen({
    super.key,
    required this.title,
    required this.loadTicket,
  });

  final String title;
  final TicketLoader loadTicket;

  @override
  State<TicketVideoScreen> createState() => _TicketVideoScreenState();
}

class _TicketVideoScreenState extends State<TicketVideoScreen> {
  VideoPlayerController? _controller;
  int? _expiresInSeconds;
  String? _error;
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  Future<void> _load() async {
    final previous = _controller;
    _controller = null;
    await previous?.dispose();
    if (!mounted) return;
    setState(() {
      _loading = true;
      _error = null;
      _expiresInSeconds = null;
    });

    try {
      final ticket = await widget.loadTicket();
      final controller =
          VideoPlayerController.networkUrl(Uri.parse(ticket.url));
      await controller.initialize();
      if (!mounted) {
        await controller.dispose();
        return;
      }
      setState(() {
        _controller = controller;
        _expiresInSeconds = ticket.expiresInSeconds;
        _loading = false;
      });
      await controller.play();
    } catch (error) {
      if (mounted) {
        setState(() {
          _loading = false;
          _error = error.toString();
        });
      }
    }
  }

  @override
  void dispose() {
    _controller?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final controller = _controller;
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.title),
        actions: [
          IconButton(
            onPressed: _loading ? null : () => unawaited(_load()),
            icon: const Icon(Icons.refresh),
            tooltip: 'Request a new ticket',
          ),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : _error != null
              ? _ErrorView(message: _error!, onRetry: () => unawaited(_load()))
              : controller == null
                  ? _ErrorView(
                      message: 'The media player is unavailable.',
                      onRetry: () => unawaited(_load()))
                  : _PlayerBody(
                      controller: controller,
                      expiresInSeconds: _expiresInSeconds),
    );
  }
}

class _PlayerBody extends StatelessWidget {
  const _PlayerBody({required this.controller, required this.expiresInSeconds});

  final VideoPlayerController controller;
  final int? expiresInSeconds;

  @override
  Widget build(BuildContext context) {
    final aspectRatio = controller.value.aspectRatio == 0
        ? 16 / 9
        : controller.value.aspectRatio;
    return Column(
      children: [
        Expanded(
          child: Center(
            child: AspectRatio(
                aspectRatio: aspectRatio, child: VideoPlayer(controller)),
          ),
        ),
        ValueListenableBuilder<VideoPlayerValue>(
          valueListenable: controller,
          builder: (context, value, child) => Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              IconButton(
                onPressed: value.isPlaying ? controller.pause : controller.play,
                icon: Icon(value.isPlaying ? Icons.pause : Icons.play_arrow),
                tooltip: value.isPlaying ? 'Pause' : 'Play',
              ),
              if (expiresInSeconds != null)
                Text('Ticket valid for ${expiresInSeconds}s',
                    style: Theme.of(context).textTheme.bodySmall),
            ],
          ),
        ),
        const SizedBox(height: 12),
      ],
    );
  }
}

class _ErrorView extends StatelessWidget {
  const _ErrorView({required this.message, required this.onRetry});

  final String message;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.error_outline, size: 42),
            const SizedBox(height: 12),
            Text(message, textAlign: TextAlign.center),
            const SizedBox(height: 16),
            FilledButton.icon(
                onPressed: onRetry,
                icon: const Icon(Icons.refresh),
                label: const Text('Try again')),
          ],
        ),
      ),
    );
  }
}
