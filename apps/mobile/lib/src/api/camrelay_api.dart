import 'dart:convert';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:http/http.dart' as http;

import 'models.dart';

class CamrelayApiException implements Exception {
  const CamrelayApiException(this.statusCode, this.message);

  final int statusCode;
  final String message;

  @override
  String toString() => 'CamrelayApiException($statusCode): $message';
}

class CamrelayApi {
  CamrelayApi({
    required String baseUrl,
    http.Client? client,
    FlutterSecureStorage? secureStorage,
  })  : _baseUrl = _normalizeBaseUrl(baseUrl),
        _client = client ?? http.Client(),
        _secureStorage = secureStorage ?? const FlutterSecureStorage();

  static const _tokenKey = 'camrelay_access_token';
  final String _baseUrl;
  final http.Client _client;
  final FlutterSecureStorage _secureStorage;

  static String _normalizeBaseUrl(String value) {
    final trimmed = value.trim();
    if (trimmed.isEmpty) {
      throw ArgumentError.value(value, 'baseUrl', 'Camrelay API base URL is required');
    }
    return trimmed.endsWith('/') ? trimmed.substring(0, trimmed.length - 1) : trimmed;
  }

  Future<void> login(String username, String password) async {
    final response = await _client.post(
      _uri('/api/v1/auth/login'),
      headers: {'content-type': 'application/json'},
      body: jsonEncode({'username': username, 'password': password}),
    );
    final payload = _decode(response);
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw CamrelayApiException(response.statusCode, _message(payload));
    }
    final token = payload['token'] as String?;
    if (token == null || token.isEmpty) {
      throw const CamrelayApiException(502, 'Login response did not contain an access token.');
    }
    await _secureStorage.write(key: _tokenKey, value: token);
  }

  Future<void> logout() async {
    try {
      await _request('POST', '/api/v1/auth/logout');
    } finally {
      await _secureStorage.delete(key: _tokenKey);
    }
  }

  Future<Principal> me() async => Principal.fromJson(await _request('GET', '/api/v1/me'));

  Future<List<CameraSummary>> cameras() async {
    final payload = await _request('GET', '/api/v1/cameras');
    return _list(payload).map(CameraSummary.fromJson).toList(growable: false);
  }

  Future<List<RecordingSummary>> recordings() async {
    final payload = await _request('GET', '/api/v1/recordings');
    return _list(payload).map(RecordingSummary.fromJson).toList(growable: false);
  }

  Future<void> startCamera(String id) async {
    await _request('POST', '/api/v1/cameras/$id/start');
  }

  Future<void> stopCamera(String id) async {
    await _request('POST', '/api/v1/cameras/$id/stop');
  }

  Future<PlaybackTicket> playbackTicket(String id) async =>
      PlaybackTicket.fromJson(await _request('POST', '/api/v1/recordings/$id/playback-ticket'));

  Future<Map<String, dynamic>> _request(String method, String path) async {
    final token = await _secureStorage.read(key: _tokenKey);
    final headers = <String, String>{'accept': 'application/json'};
    if (token != null && token.isNotEmpty) {
      headers['authorization'] = 'Bearer $token';
    }

    final response = switch (method) {
      'POST' => await _client.post(_uri(path), headers: headers),
      'GET' => await _client.get(_uri(path), headers: headers),
      _ => throw ArgumentError.value(method, 'method', 'Unsupported request method'),
    };
    final payload = _decode(response);
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw CamrelayApiException(response.statusCode, _message(payload));
    }
    return payload;
  }

  Uri _uri(String path) => Uri.parse('$_baseUrl$path');

  static Map<String, dynamic> _decode(http.Response response) {
    if (response.body.isEmpty) return <String, dynamic>{};
    final decoded = jsonDecode(response.body);
    return decoded is Map<String, dynamic> ? decoded : <String, dynamic>{'data': decoded};
  }

  static List<Map<String, dynamic>> _list(Map<String, dynamic> payload) {
    final data = payload['data'];
    if (data is List) {
      return data.whereType<Map<String, dynamic>>().toList(growable: false);
    }
    return <Map<String, dynamic>>[];
  }

  static String _message(Map<String, dynamic> payload) =>
      payload['message'] as String? ?? payload['error'] as String? ?? 'Camrelay request failed.';
}
