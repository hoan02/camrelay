import 'package:flutter/material.dart';

import 'api/camrelay_api.dart';
import 'api/models.dart';
import 'media/ticket_video_screen.dart';

class CamrelayApp extends StatefulWidget {
  const CamrelayApp({super.key, required this.api});

  final CamrelayApi api;

  @override
  State<CamrelayApp> createState() => _CamrelayAppState();
}

class _CamrelayAppState extends State<CamrelayApp> {
  bool _signedIn = false;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Camrelay',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        brightness: Brightness.dark,
        colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xffd8ff62), brightness: Brightness.dark),
        useMaterial3: true,
        scaffoldBackgroundColor: const Color(0xff101314),
      ),
      home: _signedIn
          ? HomeScreen(api: widget.api, onSignOut: () async { await widget.api.logout(); setState(() => _signedIn = false); })
          : LoginScreen(api: widget.api, onSignedIn: () => setState(() => _signedIn = true)),
    );
  }
}

class LoginScreen extends StatefulWidget {
  const LoginScreen({super.key, required this.api, required this.onSignedIn});

  final CamrelayApi api;
  final VoidCallback onSignedIn;

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  final _username = TextEditingController();
  final _password = TextEditingController();
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _username.dispose();
    _password.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    setState(() { _busy = true; _error = null; });
    try {
      await widget.api.login(_username.text.trim(), _password.text);
      widget.onSignedIn();
    } on CamrelayApiException catch (error) {
      setState(() => _error = error.message);
    } catch (_) {
      setState(() => _error = 'Camrelay is not reachable.');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 420),
            child: Padding(
              padding: const EdgeInsets.all(28),
              child: Card(
                child: Padding(
                  padding: const EdgeInsets.all(24),
                  child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                    Text('camrelay', style: Theme.of(context).textTheme.headlineMedium?.copyWith(fontWeight: FontWeight.w700)),
                    const SizedBox(height: 8),
                    Text('Private camera gateway', style: Theme.of(context).textTheme.bodyMedium),
                    const SizedBox(height: 28),
                    TextField(controller: _username, autofillHints: const [AutofillHints.username], decoration: const InputDecoration(labelText: 'Username')),
                    const SizedBox(height: 14),
                    TextField(controller: _password, obscureText: true, autofillHints: const [AutofillHints.password], decoration: const InputDecoration(labelText: 'Password'), onSubmitted: (_) => _submit()),
                    if (_error != null) ...[
                      const SizedBox(height: 14),
                      Text(_error!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
                    ],
                    const SizedBox(height: 22),
                    FilledButton(onPressed: _busy ? null : _submit, child: Text(_busy ? 'Connecting…' : 'Enter console')),
                  ]),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.api, required this.onSignOut});

  final CamrelayApi api;
  final VoidCallback onSignOut;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  late Future<Principal> _principal;
  late Future<List<CameraSummary>> _cameras;
  late Future<List<RecordingSummary>> _recordings;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  void _reload() {
    _principal = widget.api.me();
    _cameras = widget.api.cameras();
    _recordings = widget.api.recordings();
  }

  Future<void> _toggle(CameraSummary camera) async {
    // The API returns lifecycle state from /tunnels; this foundation only
    // exposes the explicit action. A richer status model will drive this UI
    // once the live-media contract is finalized.
    try {
      await widget.api.startCamera(camera.id);
      if (mounted) setState(_reload);
    } on CamrelayApiException catch (error) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(error.message)));
    }
  }

  Future<void> _openLive(CameraSummary camera) async {
    await Navigator.of(context).push<void>(
      MaterialPageRoute<void>(
        builder: (_) => TicketVideoScreen(
          title: camera.name,
          loadTicket: () async {
            final ticket = await widget.api.liveTicket(camera.id);
            return MediaTicketData(
              url: widget.api.resolveUrl(ticket.url),
              expiresInSeconds: ticket.expiresInSeconds,
            );
          },
        ),
      ),
    );
  }

  Future<void> _openRecording(RecordingSummary recording) async {
    await Navigator.of(context).push<void>(
      MaterialPageRoute<void>(
        builder: (_) => TicketVideoScreen(
          title: recording.cameraName,
          loadTicket: () async {
            final ticket = await widget.api.playbackTicket(recording.id);
            return MediaTicketData(
              url: widget.api.resolveUrl(ticket.url),
              expiresInSeconds: ticket.expiresInSeconds,
            );
          },
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Camrelay'), actions: [IconButton(onPressed: widget.onSignOut, icon: const Icon(Icons.logout), tooltip: 'Sign out')]),
      body: RefreshIndicator(
        onRefresh: () async => setState(_reload),
        child: ListView(padding: const EdgeInsets.fromLTRB(18, 18, 18, 32), children: [
          FutureBuilder<Principal>(future: _principal, builder: (context, snapshot) => Text(snapshot.data == null ? 'Control plane' : 'Hello, ${snapshot.data!.username}', style: Theme.of(context).textTheme.headlineSmall)),
          const SizedBox(height: 24),
          Text('Cameras', style: Theme.of(context).textTheme.titleLarge),
          const SizedBox(height: 8),
          FutureBuilder<List<CameraSummary>>(future: _cameras, builder: (context, snapshot) {
            if (snapshot.hasError) return const _ErrorTile(message: 'Camera API is unavailable.');
            if (!snapshot.hasData) return const Center(child: Padding(padding: EdgeInsets.all(20), child: CircularProgressIndicator()));
            if (snapshot.data!.isEmpty) return const _EmptyTile(message: 'No cameras configured yet.');
            return Column(children: snapshot.data!.map((camera) => Card(child: ListTile(leading: const Icon(Icons.videocam_outlined), title: Text(camera.name), subtitle: Text('${camera.brand} · ${camera.serial}'), trailing: Wrap(children: [IconButton(icon: const Icon(Icons.live_tv_outlined), tooltip: 'Watch live', onPressed: () => _openLive(camera)), IconButton(icon: const Icon(Icons.play_arrow), tooltip: 'Start relay', onPressed: () => _toggle(camera))]))).toList());
          }),
          const SizedBox(height: 24),
          Text('Recent recordings', style: Theme.of(context).textTheme.titleLarge),
          const SizedBox(height: 8),
          FutureBuilder<List<RecordingSummary>>(future: _recordings, builder: (context, snapshot) {
            if (snapshot.hasError) return const _ErrorTile(message: 'Recording API is unavailable.');
            if (!snapshot.hasData) return const Center(child: Padding(padding: EdgeInsets.all(20), child: CircularProgressIndicator()));
            if (snapshot.data!.isEmpty) return const _EmptyTile(message: 'No recordings yet.');
            return Column(children: snapshot.data!.take(10).map((recording) => ListTile(title: Text(recording.cameraName), subtitle: Text(recording.startedAt.toLocal().toString()), trailing: Wrap(crossAxisAlignment: WrapCrossAlignment.center, children: [Text(recording.status), IconButton(icon: const Icon(Icons.play_circle_outline), tooltip: 'Play recording', onPressed: () => _openRecording(recording))]))).toList());
          }),
        ]),
      ),
    );
  }
}

class _EmptyTile extends StatelessWidget {
  const _EmptyTile({required this.message});
  final String message;
  @override
  Widget build(BuildContext context) => Card(child: Padding(padding: const EdgeInsets.all(18), child: Text(message)));
}

class _ErrorTile extends StatelessWidget {
  const _ErrorTile({required this.message});
  final String message;
  @override
  Widget build(BuildContext context) => Card(child: Padding(padding: const EdgeInsets.all(18), child: Text(message, style: TextStyle(color: Theme.of(context).colorScheme.error))));
}
