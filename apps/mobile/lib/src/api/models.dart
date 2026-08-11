class Principal {
  const Principal({
    required this.username,
    required this.role,
    required this.authType,
  });

  final String username;
  final String role;
  final String authType;

  factory Principal.fromJson(Map<String, dynamic> json) => Principal(
        username: json['username'] as String? ?? '',
        role: json['role'] as String? ?? 'viewer',
        authType: json['auth_type'] as String? ?? 'session',
      );
}

class CameraSummary {
  const CameraSummary({
    required this.id,
    required this.name,
    required this.brand,
    required this.serial,
    required this.localPort,
    required this.autoStart,
  });

  final String id;
  final String name;
  final String brand;
  final String serial;
  final int localPort;
  final bool autoStart;

  factory CameraSummary.fromJson(Map<String, dynamic> json) => CameraSummary(
        id: json['id'] as String? ?? '',
        name: json['name'] as String? ?? '',
        brand: json['brand'] as String? ?? '',
        serial: json['serial'] as String? ?? '',
        localPort: (json['local_port'] as num?)?.toInt() ?? 0,
        autoStart: json['auto_start'] as bool? ?? false,
      );
}

class TunnelSummary {
  const TunnelSummary({required this.id, required this.status});

  final String id;
  final String status;

  factory TunnelSummary.fromJson(Map<String, dynamic> json) => TunnelSummary(
        id: json['id'] as String? ?? '',
        status: json['status'] as String? ?? 'stopped',
      );
}

class RecordingSummary {
  const RecordingSummary({
    required this.id,
    required this.cameraId,
    required this.cameraName,
    required this.startedAt,
    required this.endedAt,
    required this.kind,
    required this.bytes,
    required this.status,
    required this.archiveAvailable,
    required this.checksumSha256,
    required this.archiveVerified,
  });

  final String id;
  final String cameraId;
  final String cameraName;
  final DateTime startedAt;
  final DateTime? endedAt;
  final String kind;
  final int bytes;
  final String status;
  final bool archiveAvailable;
  final String? checksumSha256;
  final bool archiveVerified;

  factory RecordingSummary.fromJson(Map<String, dynamic> json) =>
      RecordingSummary(
        id: json['id'] as String? ?? '',
        cameraId: json['camera_id'] as String? ?? '',
        cameraName: json['camera_name'] as String? ?? '',
        startedAt: DateTime.tryParse(json['started_at'] as String? ?? '') ??
            DateTime.fromMillisecondsSinceEpoch(0),
        endedAt: DateTime.tryParse(json['ended_at'] as String? ?? ''),
        kind: json['kind'] as String? ?? 'segment',
        bytes: (json['bytes'] as num?)?.toInt() ?? 0,
        status: json['status'] as String? ?? 'unknown',
        archiveAvailable: json['archive_available'] as bool? ?? false,
        checksumSha256: json['checksum_sha256'] as String?,
        archiveVerified: json['archive_verified'] as bool? ?? false,
      );
}

class EventSummary {
  const EventSummary({
    required this.id,
    required this.kind,
    required this.cameraId,
    required this.cameraName,
    required this.occurredAt,
    required this.severity,
    required this.message,
    required this.source,
  });

  final String id;
  final String kind;
  final String cameraId;
  final String cameraName;
  final DateTime occurredAt;
  final String severity;
  final String message;
  final String source;

  factory EventSummary.fromJson(Map<String, dynamic> json) => EventSummary(
        id: json['id'] as String? ?? '',
        kind: json['kind'] as String? ?? 'activity',
        cameraId: json['camera_id'] as String? ?? '',
        cameraName: json['camera_name'] as String? ?? '',
        occurredAt: DateTime.tryParse(json['occurred_at'] as String? ?? '') ??
            DateTime.fromMillisecondsSinceEpoch(0),
        severity: json['severity'] as String? ?? 'info',
        message: json['message'] as String? ?? '',
        source: json['source'] as String? ?? 'unknown',
      );
}

class PlaybackTicket {
  const PlaybackTicket({required this.url, required this.expiresInSeconds});

  final String url;
  final int expiresInSeconds;

  factory PlaybackTicket.fromJson(Map<String, dynamic> json) => PlaybackTicket(
        url: json['url'] as String? ?? '',
        expiresInSeconds: (json['expires_in_seconds'] as num?)?.toInt() ?? 0,
      );
}

class LiveTicket {
  const LiveTicket(
      {required this.protocol,
      required this.url,
      required this.expiresInSeconds});

  final String protocol;
  final String url;
  final int expiresInSeconds;

  factory LiveTicket.fromJson(Map<String, dynamic> json) => LiveTicket(
        protocol: json['protocol'] as String? ?? 'hls',
        url: json['url'] as String? ?? '',
        expiresInSeconds: (json['expires_in_seconds'] as num?)?.toInt() ?? 0,
      );
}
