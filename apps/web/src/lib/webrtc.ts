/**
 * Browser WHEP adapter for the ticket returned by Camrelay.
 *
 * It deliberately accepts a Camrelay ticket URL only. Camera RTSP URLs,
 * MediaMTX locations, API bearer tokens, and provider credentials never enter
 * this client.
 */
export type WebRtcConnection = {
  peer: RTCPeerConnection;
  close: () => Promise<void>;
};

function waitForIceGathering(peer: RTCPeerConnection): Promise<void> {
  if (peer.iceGatheringState === "complete") return Promise.resolve();
  return new Promise(resolve => {
    const onStateChange = () => {
      if (peer.iceGatheringState === "complete") {
        peer.removeEventListener("icegatheringstatechange", onStateChange);
        resolve();
      }
    };
    peer.addEventListener("icegatheringstatechange", onStateChange);
  });
}

export async function connectWhep(
  ticketUrl: string,
  signal?: AbortSignal,
): Promise<WebRtcConnection> {
  const peer = new RTCPeerConnection();
  peer.addTransceiver("video", { direction: "recvonly" });
  peer.addTransceiver("audio", { direction: "recvonly" });

  try {
    const offer = await peer.createOffer();
    await peer.setLocalDescription(offer);
    await waitForIceGathering(peer);
    const localSdp = peer.localDescription?.sdp;
    if (!localSdp) throw new Error("WebRTC offer was not created.");

    const response = await fetch(ticketUrl, {
      method: "POST",
      headers: { Accept: "application/sdp", "Content-Type": "application/sdp" },
      body: localSdp,
      signal,
    });
    if (!response.ok) throw new Error(`WebRTC negotiation failed (${response.status}).`);
    const answer = await response.text();
    await peer.setRemoteDescription({ type: "answer", sdp: answer });
    const sessionUrl = response.headers.get("Location");
    if (!sessionUrl) throw new Error("WebRTC gateway did not return a session location.");

    return {
      peer,
      close: async () => {
        peer.close();
        await fetch(sessionUrl, { method: "DELETE", credentials: "same-origin" }).catch(() => undefined);
      },
    };
  } catch (error) {
    peer.close();
    throw error;
  }
}
