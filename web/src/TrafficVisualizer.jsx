import React, { useState, useEffect, useRef, useMemo } from 'react';
import { Canvas, useFrame, useThree } from '@react-three/fiber';
import { OrbitControls, Text, PerspectiveCamera, Sparkles, Grid, Html } from '@react-three/drei';
import { EffectComposer, Bloom, Vignette, Scanline } from '@react-three/postprocessing';
import * as THREE from 'three';

// --- Constants ---
const PEER_RADIUS = 15;
const TRAFFIC_SPEED = 0.5;
// const PEER_TIMEOUT = 60000; // 1 minute (Moved to Config)
const AGENT_CLUSTER_RADIUS = 4; // Radius for multiple agents

// --- Helper: Generate Position on Circle ---
const getPosition = (index, total) => {
  const angle = (index / total) * Math.PI * 2;
  return new THREE.Vector3(Math.cos(angle) * PEER_RADIUS, 0, Math.sin(angle) * PEER_RADIUS);
};

// --- Components ---

function Agent({ label, position, isSelected, onClick }) {
  const groupRef = useRef();
  const coreRef = useRef();
  const shell1Ref = useRef();
  const shell2Ref = useRef();
  const ringRef = useRef();
  const [hovered, setHover] = useState(false);

  useFrame((state, delta) => {
    const t = state.clock.getElapsedTime();

    // Bobbing
    if (groupRef.current) {
      // Use passed position Y + bobbing
      groupRef.current.position.y = (position?.y || 2.0) + Math.sin(t * 1) * 0.2;
    }

    // Core Pulse
    if (coreRef.current) {
      const scale = 1 + Math.sin(t * 3) * 0.1;
      coreRef.current.scale.setScalar(scale);
    }

    // Shell Rotations (Multi-axis)
    if (shell1Ref.current) {
      shell1Ref.current.rotation.x += delta * 0.2;
      shell1Ref.current.rotation.y += delta * 0.3;
    }
    if (shell2Ref.current) {
      shell2Ref.current.rotation.x -= delta * 0.1;
      shell2Ref.current.rotation.z += delta * 0.2;
    }

    // Ring Rotation
    if (ringRef.current) {
      ringRef.current.rotation.z -= delta * 0.5;
    }
  });

  const glowColor = isSelected ? "#ffee00" : "#00ffff"; // Gold if selected, else Cyan

  return (
    <group ref={groupRef} position={position || [0, 2, 0]}>
      {/* Interactive Hit Area */}
      <mesh
        visible={false}
        onClick={(e) => { e.stopPropagation(); onClick(label); }}
        onPointerOver={() => setHover(true)}
        onPointerOut={() => setHover(false)}
      >
        {/* Bigger hit area for Agent */}
        <sphereGeometry args={[2.5, 16, 16]} />
        <meshBasicMaterial />
      </mesh>

      {/* Core Sphere (Glowing) */}
      <mesh ref={coreRef}>
        <sphereGeometry args={[0.8, 32, 32]} />
        <meshStandardMaterial
          color={glowColor}
          emissive={glowColor}
          emissiveIntensity={hovered ? 3 : 2}
          toneMapped={false}
        />
        <pointLight color={glowColor} intensity={2} distance={5} />
      </mesh>

      {/* Wireframe Shell 1 */}
      <mesh ref={shell1Ref}>
        <icosahedronGeometry args={[1.2, 1]} />
        <meshBasicMaterial color="#0088ff" wireframe transparent opacity={0.3} />
      </mesh>

      {/* Wireframe Shell 2 */}
      <mesh ref={shell2Ref}>
        <icosahedronGeometry args={[1.4, 0]} />
        <meshBasicMaterial color="#ffffff" wireframe transparent opacity={0.1} />
      </mesh>

      {/* Orbiting Ring System (Flat on XZ plane initially) */}
      <group rotation={[Math.PI / 2, 0, 0]}>
        <mesh ref={ringRef}>
          <ringGeometry args={[1.8, 1.9, 64, 1, 0, Math.PI * 1.5]} />
          <meshBasicMaterial color={glowColor} side={THREE.DoubleSide} transparent opacity={0.5} />
        </mesh>
        <mesh rotation={[0, 0, Math.PI]}>
          <ringGeometry args={[1.6, 1.65, 64, 1, 0, Math.PI]} />
          <meshBasicMaterial color="#0088ff" side={THREE.DoubleSide} transparent opacity={0.3} />
        </mesh>
      </group>

      {/* Base "Magic Circle" on floor */}
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -1.5, 0]}>
        <ringGeometry args={[2, 2.2, 64]} />
        <meshBasicMaterial color={glowColor} transparent opacity={0.2} />
      </mesh>

      <Text position={[0, 2.5, 0]} fontSize={0.5} color={glowColor} anchorX="center" anchorY="middle">
        {label || "Agent"}
      </Text>
    </group>
  );
}

// --- Helper: Check for Private IP ---
const isPrivateIP = (ip) => {
  // IPv4 Private Ranges
  // 10.0.0.0/8
  // 172.16.0.0/12
  // 192.168.0.0/16
  // 127.0.0.0/8 (Loopback)
  if (ip.startsWith("10.") || ip.startsWith("127.")) return true;
  if (ip.startsWith("192.168.")) return true;

  if (ip.startsWith("172.")) {
    const secondOctet = parseInt(ip.split('.')[1], 10);
    if (secondOctet >= 16 && secondOctet <= 31) return true;
  }

  // IPv6 Private/Local Ranges
  // fc00::/7 (Unique Local) -> starts with fc or fd
  // fe80::/10 (Link Local) -> starts with fe8, fe9, fea, feb
  const lowerIp = ip.toLowerCase();
  if (lowerIp.startsWith("fc") || lowerIp.startsWith("fd")) return true;
  if (lowerIp.startsWith("fe8") || lowerIp.startsWith("fe9") || lowerIp.startsWith("fea") || lowerIp.startsWith("feb")) return true;
  if (lowerIp === "::1") return true;

  return false;
};

function Peer({ ip, position, isHot, isSelected, onClick, onDragStart, onDragEnd, onDrag }) {
  const groupRef = useRef();
  const ring1Ref = useRef();
  const ring2Ref = useRef();
  const ring3Ref = useRef();
  const decoRef = useRef();
  const [hovered, setHover] = useState(false);

  // Color Logic
  // Priority: Selected > Hot > Internet (Orange) > Intranet (Blue)
  let baseColor = "#0088ff"; // Default Blue (Intranet)

  if (!isPrivateIP(ip)) {
    baseColor = "#ff8800"; // Orange (Internet)
  }

  if (isHot) baseColor = "#ff0000"; // Red (High Traffic)
  if (isSelected) baseColor = "#ffcc00"; // Gold (Selected)

  const glowColor = isSelected ? "#ffee00" : baseColor;

  useFrame((state, delta) => {
    if (groupRef.current) {
      // Look away from center (0,0,0)
      const mysPos = groupRef.current.position;
      const target = new THREE.Vector3(mysPos.x * 2, mysPos.y, mysPos.z * 2);
      groupRef.current.lookAt(target);
    }

    // GITS Style Rotation
    if (ring1Ref.current) ring1Ref.current.rotation.z -= delta * 0.5;
    if (ring2Ref.current) ring2Ref.current.rotation.z += delta * 0.3;
    if (ring3Ref.current) ring3Ref.current.rotation.z -= delta * 1.0;

    // Deco Rotation
    if (decoRef.current) {
      decoRef.current.rotation.z += delta * 0.2;
      decoRef.current.rotation.x += delta * 0.1;
    }
  });

  return (
    <group ref={groupRef} position={position}>
      {/* Interactive Hit Area */}
      <mesh
        visible={false}
        rotation={[Math.PI / 2, 0, 0]}
        onClick={(e) => { e.stopPropagation(); onClick(ip); }}
        onPointerOver={() => setHover(true)}
        onPointerOut={() => setHover(false)}
        onPointerDown={(e) => {
          e.stopPropagation();
          e.target.setPointerCapture(e.pointerId);
          onDragStart(ip);
        }}
        onPointerUp={(e) => {
          e.stopPropagation();
          e.target.releasePointerCapture(e.pointerId);
          onDragEnd();
        }}
        onPointerMove={(e) => {
          if (e.buttons) { // dragging
            onDrag(e.point);
          }
        }}
      >
        <cylinderGeometry args={[2, 2, 1, 16]} />
        <meshBasicMaterial />
      </mesh>

      {/* Visual Representation: Flat Disc (Standing Up) */}
      <group rotation={[Math.PI / 2, 0, 0]}>
        {/* Main Disc Body (Transparent Glassy) */}
        <mesh position={[0, 0, 0]}>
          <cylinderGeometry args={[1.5, 1.5, 0.05, 32]} />
          <meshStandardMaterial
            color="#001122"
            emissive={baseColor}
            emissiveIntensity={hovered ? 0.5 : 0.2}
            transparent
            opacity={0.3}
            roughness={0.1}
            metalness={0.8}
          />
        </mesh>

        {/* Decoration Wireframe (Tech Core) */}
        <mesh ref={decoRef} position={[0, 0, 0]}>
          <octahedronGeometry args={[1.1, 0]} />
          <meshBasicMaterial color={baseColor} wireframe transparent opacity={0.15} />
        </mesh>

        {/* Outer Ring (Static) */}
        <mesh position={[0, 0.03, 0]}>
          <ringGeometry args={[1.5, 1.55, 64]} />
          <meshBasicMaterial color={glowColor} side={THREE.DoubleSide} transparent opacity={0.8} />
        </mesh>

        {/* Rotating Arc 1 (Slow, Large) */}
        <mesh ref={ring1Ref} position={[0, 0.04, 0]}>
          <ringGeometry args={[1.3, 1.4, 32, 1, 0, Math.PI * 1.5]} />
          <meshBasicMaterial color={baseColor} transparent opacity={0.4} side={THREE.DoubleSide} />
        </mesh>

        {/* Rotating Arc 2 (Counter-rotate, Medium) */}
        <mesh ref={ring2Ref} position={[0, 0.05, 0]} rotation={[0, 0, 1]}>
          <ringGeometry args={[1.0, 1.1, 32, 1, 0, Math.PI]} />
          <meshBasicMaterial color={baseColor} transparent opacity={0.3} side={THREE.DoubleSide} />
        </mesh>

        {/* Rotating Arc 3 (Fast, Small bits) */}
        <mesh ref={ring3Ref} position={[0, 0.06, 0]}>
          <ringGeometry args={[0.7, 0.8, 16, 1, 0, Math.PI * 0.5]} />
          <meshBasicMaterial color={glowColor} transparent opacity={0.5} side={THREE.DoubleSide} />
        </mesh>

        {/* Text Label (IP) */}
        <Text
          position={[0, 0.5, 0]} // Lifted up slightly
          rotation={[-Math.PI / 2, 0, 0]} // Face UP (local Y) -> Face Target (World Z)
          fontSize={0.4}
          color={isSelected ? "#ffffff" : "#cccccc"}
          anchorX="center"
          anchorY="middle"
        >
          {ip}
        </Text>
      </group>
    </group>
  );
}

const DECAY_RATE = 2.0;

function ConnectionLink({ linkData }) {
  const ref = useRef();
  // Fixed thinness (Reduced)
  const THICKNESS = 0.05;

  useFrame((state, delta) => {
    if (ref.current && linkData) {
      // Time-based decay
      linkData.volume = Math.max(0, linkData.volume - (linkData.volume * DECAY_RATE * delta));

      ref.current.scale.x = THICKNESS;
      ref.current.scale.z = THICKNESS;

      // Update Position and Orientation
      const start = linkData.start;
      const end = linkData.end;
      const dist = start.distanceTo(end);

      ref.current.position.lerpVectors(start, end, 0.5);
      ref.current.scale.y = dist;

      const direction = new THREE.Vector3().subVectors(end, start).normalize();
      const up = new THREE.Vector3(0, 1, 0);
      const quaternion = new THREE.Quaternion().setFromUnitVectors(up, direction);
      ref.current.setRotationFromQuaternion(quaternion);

      // COLOR / INTENSITY Change logic
      const ratio = Math.min(1.0, linkData.volume / 10000);

      // Color Interpolation
      const color = new THREE.Color().lerpColors(
        new THREE.Color("#00ffff"),
        new THREE.Color("#ff0000"),
        ratio
      );

      ref.current.material.color = color;
      ref.current.material.emissive = color;
      ref.current.material.emissiveIntensity = 1 + ratio * 10;
    }
  });

  return (
    <mesh ref={ref}>
      <cylinderGeometry args={[1, 1, 1, 8]} />
      <meshStandardMaterial toneMapped={false} transparent opacity={0.6} />
    </mesh>
  )
}

function DespawnEffect({ position, onComplete }) {
  const ref = useRef();
  const life = useRef(1.0); // 1.0 to 0.0

  useFrame((state, delta) => {
    life.current -= delta * 2.0; // Die in 0.5s
    if (life.current <= 0) {
      onComplete();
      return;
    }

    if (ref.current) {
      const scale = 1 + (1 - life.current) * 3; // Expand 1 -> 4
      ref.current.scale.setScalar(scale);
      ref.current.material.opacity = life.current;
    }
  });

  return (
    <mesh ref={ref} position={position} rotation={[-Math.PI / 2, 0, 0]}>
      <ringGeometry args={[1, 1.2, 32]} />
      <meshBasicMaterial color="#ff0055" transparent side={THREE.DoubleSide} />
    </mesh>
  );
}

// --- Info Panel Component ---
// --- Info Panel Component ---
function InfoPanel({ peerIp, peerData, agentIp, agentData, onClose, geoipEnabled, attribution }) {
  const [details, setDetails] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  const targetIp = peerIp || agentIp;
  const isAgent = !!agentIp;

  const fetchDetails = (ip) => {
    setLoading(true);
    setError(null);
    setDetails(null);

    const url = geoipEnabled
      ? `/geoip/${ip}`
      : `https://ipapi.co/${ip}/json/`;

    fetch(url)
      .then(res => {
        if (!res.ok) throw new Error(res.statusText || 'Fetch failed');
        return res.json();
      })
      .then(data => {
        if (data.error) throw new Error(data.error);
        setDetails(data);
        setLoading(false);
      })
      .catch(err => {
        console.error("GeoIP Error", err);
        setError("Failed to fetch details");
        setLoading(false);
      });
  };

  // Reset state when targetIp changes, and auto-fetch if enabled
  useEffect(() => {
    setDetails(null);
    setLoading(false);
    setError(null);

    if (geoipEnabled && !isPrivateIP(targetIp)) {
      fetchDetails(targetIp);
    }
  }, [targetIp, geoipEnabled]);

  if (!targetIp) return null;

  // Render ports logic
  const renderPorts = (pData) => {
    if (!pData) return 'N/A';
    const ins = pData.portsIn ? Array.from(pData.portsIn) : [];
    const outs = pData.portsOut ? Array.from(pData.portsOut) : [];

    if (ins.length === 0 && outs.length === 0) return 'N/A';

    return (
      <div style={{ fontSize: '0.9em' }}>
        {ins.length > 0 && <div><span style={{ color: '#00ffcc' }}>IN:</span> {ins.join(', ')}</div>}
        {outs.length > 0 && <div><span style={{ color: '#ffcc00' }}>OUT:</span> {outs.join(', ')}</div>}
      </div>
    );
  };

  return (
    <div style={{
      position: 'absolute',
      top: '20px',
      right: '20px',
      width: '300px',
      background: 'rgba(0, 10, 20, 0.9)',
      border: `1px solid ${isAgent ? '#ffee00' : '#00ffcc'}`,
      color: isAgent ? '#ffee00' : '#00ffcc',
      padding: '20px',
      fontFamily: 'monospace',
      borderRadius: '8px',
      backdropFilter: 'blur(5px)',
      zIndex: 1000
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '10px' }}>
        <h3 style={{ margin: 0 }}>{isAgent ? 'AGENT DETAILS' : 'PEER DETAILS'}</h3>
        <button onClick={onClose} style={{ background: 'transparent', border: 'none', color: '#ff0055', cursor: 'pointer', fontSize: '16px' }}>X</button>
      </div>

      <div><strong>IP:</strong> {targetIp}</div>
      {/* For Agent, maybe we don't have volume/protocols logic wired up effectively yet in visualizer state for 'agentsRef' */}
      <div><strong>Last Seen:</strong> {new Date((peerData || agentData)?.lastSeen).toLocaleTimeString()}</div>

      {!isAgent && (
        <>
          <div><strong>Speed:</strong> {((peerData?.volume || 0) * 2 / 1024).toFixed(2)} KB/s</div>
          <div><strong>Protocols:</strong> {peerData?.protocols ? Array.from(peerData.protocols).join(', ') : 'N/A'}</div>
          <div><strong>Ports:</strong> {renderPorts(peerData)}</div>
        </>
      )}

      {isAgent && (
        <div style={{ marginTop: '10px', fontStyle: 'italic', color: '#cccc00' }}>
          Active Agent Node
        </div>
      )}

      <hr style={{ borderColor: '#004444' }} />

      {!details && !loading && !error && !isPrivateIP(targetIp) && !geoipEnabled && (
        <button
          onClick={() => fetchDetails(targetIp)}
          style={{
            width: '100%',
            padding: '8px',
            background: '#0a4a4a',
            color: '#00ffcc',
            border: '1px solid #00ffcc',
            cursor: 'pointer',
            marginTop: '10px'
          }}
        >
          Fetch GeoIP Info
        </button>
      )}

      {loading && <div>Loading GeoIP...</div>}

      {details && (
        <>
          <div><strong>Country:</strong> {details.country_name}</div>
          <div><strong>City:</strong> {details.city}</div>
          <div><strong>ISP:</strong> {details.org}</div>
          <div><strong>ASN:</strong> {details.asn}</div>
        </>
      )}
      {error && <div style={{ color: 'red' }}>{error}</div>}

      {/* Attribution */}
      {attribution && attribution.text && (
        <div style={{ marginTop: '15px', fontSize: '0.8em', textAlign: 'center', opacity: 0.8 }}>
          {attribution.url ? (
            <a href={attribution.url} target="_blank" rel="noopener noreferrer" style={{ color: '#00ffcc', textDecoration: 'none' }}>
              {attribution.text}
            </a>
          ) : (
            <span style={{ color: '#00ffcc' }}>{attribution.text}</span>
          )}
        </div>
      )}
    </div>
  );
}



import { retry, delay, repeat } from 'rxjs/operators';
import { timer } from 'rxjs';
import { AgentServiceClientImpl, Empty, GrpcWebImpl } from './proto/packet';

export default function TrafficVisualizer() {
  // useRefs for data (Source of Truth)
  const peersRef = useRef({});
  const agentsRef = useRef({}); // Store agents data
  const linksRef = useRef({});
  const timeoutRef = useRef(30000); // Default 30s, updated by config
  // const ws = useRef(null); // No longer needed

  // State
  const [peersState, setPeersState] = useState({});
  const [agentsState, setAgentsState] = useState({}); // Renderable agents
  const [linksState, setLinksState] = useState({});
  const [despawnEffects, setDespawnEffects] = useState([]);
  const [geoipEnabled, setGeoipEnabled] = useState(false);
  const [attribution, setAttribution] = useState(null);

  // Interaction State
  const [selectedPeer, setSelectedPeer] = useState(null);
  const [selectedAgent, setSelectedAgent] = useState(null);
  const [draggingTarget, setDraggingTarget] = useState(null); // { id: string, type: 'peer' | 'agent' }
  const orbitRef = useRef();


  const removeEffect = (id) => {
    setDespawnEffects(prev => prev.filter(e => e.id !== id));
  };

  useEffect(() => {
    const init = async () => {
      try {
        const configRes = await fetch('/config');
        const config = await configRes.json();

        // gRPC-Web Setup
        const protocol = window.location.protocol === 'https:' ? 'https:' : 'http:';
        const host = window.location.hostname;
        // Use fetched port
        // Use fetched port
        const port = config.grpcPort || '50051';
        if (config.peerTimeout) {
          timeoutRef.current = config.peerTimeout;
        }
        setGeoipEnabled(config.geoipEnabled || false);
        setAttribution({ text: config.geoipAttributionText, url: config.geoipAttributionUrl });
        const serverUrl = `${protocol}//${host}:${port}`;

        console.log('Connecting to gRPC-Web Server at', serverUrl);

        const rpc = new GrpcWebImpl(serverUrl, { debug: false });
        const client = new AgentServiceClientImpl(rpc);

        // Subscribe with Retry Logic
        const sub = client.Subscribe(Empty.create({}))
          .pipe(
            retry({
              delay: (errors) => errors.pipe(delay(2000))
            }),
            repeat({ delay: 3000 })
          )
          .subscribe({
            next: (data) => {
              // data is Packet object directly
              if (data.type !== 'traffic') return;

              const timestamp = Date.now();

              // Identity Check - Agents
              const isLoopback = (ip) => ip === '127.0.0.1' || ip === '::1' || ip === 'localhost';

              const registerAgent = (ip) => {
                if (!ip || isLoopback(ip) || ip === "AGENT") return;

                // Promote Peer to Agent if needed
                if (peersRef.current[ip]) {
                  delete peersRef.current[ip];
                  if (selectedPeer === ip) setSelectedPeer(null);
                  setPeersState({ ...peersRef.current });
                }

                if (!agentsRef.current[ip]) {
                  agentsRef.current[ip] = {
                    ip: ip,
                    lastSeen: timestamp,
                    position: new THREE.Vector3(0, 2, 0) // Default
                  };
                } else {
                  agentsRef.current[ip].lastSeen = timestamp;
                }
              };

              if (data.srcIsAgent) registerAgent(data.srcIp);
              if (data.dstIsAgent) registerAgent(data.dstIp);

              // Update Peers
              const currentPeers = peersRef.current;
              const processPeer = (ip, isAgent, role) => {
                if (!ip) return;
                if (isAgent) return;
                if (agentsRef.current[ip]) return;
                if (ip === "AGENT" || ip === "127.0.0.1" || ip === "localhost") return;

                if (!currentPeers[ip]) {
                  const angle = Math.random() * Math.PI * 2;
                  currentPeers[ip] = {
                    position: new THREE.Vector3(Math.cos(angle) * PEER_RADIUS, Math.random() * 5 - 2, Math.sin(angle) * PEER_RADIUS),
                    lastSeen: timestamp,
                    volume: 0,
                    protocols: new Set(),
                    portsIn: new Set(),
                    portsOut: new Set()
                  };
                  setPeersState({ ...currentPeers });
                } else {
                  currentPeers[ip].lastSeen = timestamp;
                }
                if (currentPeers[ip]) {
                  currentPeers[ip].volume += data.size;
                  if (data.proto) currentPeers[ip].protocols.add(data.proto);
                  if (role === 'src') {
                    if (data.dstPort && data.dstPort !== 0) currentPeers[ip].portsOut.add(data.dstPort);
                  } else {
                    if (data.dstPort && data.dstPort !== 0) currentPeers[ip].portsIn.add(data.dstPort);
                  }
                  if (currentPeers[ip].protocols.size > 5) currentPeers[ip].protocols = new Set(Array.from(currentPeers[ip].protocols).slice(-5));
                  if (currentPeers[ip].portsIn.size > 5) currentPeers[ip].portsIn = new Set(Array.from(currentPeers[ip].portsIn).slice(-5));
                  if (currentPeers[ip].portsOut.size > 5) currentPeers[ip].portsOut = new Set(Array.from(currentPeers[ip].portsOut).slice(-5));
                }
              }

              processPeer(data.srcIp, data.srcIsAgent, 'src');
              processPeer(data.dstIp, data.dstIsAgent, 'dst');

              // Traffic Links
              const getPos = (ip) => {
                if (agentsRef.current[ip]) return agentsRef.current[ip].position;
                if (ip === "AGENT" || ip === "127.0.0.1" || ip === "localhost") return new THREE.Vector3(0, 2, 0);
                return currentPeers[ip]?.position || new THREE.Vector3(0, 0, 0);
              };

              const srcPos = getPos(data.srcIp);
              const dstPos = getPos(data.dstIp);

              if (srcPos && dstPos && !srcPos.equals(dstPos)) {
                const ids = [data.srcIp, data.dstIp].sort();
                const linkId = ids.join('-');

                if (!linksRef.current[linkId]) {
                  linksRef.current[linkId] = {
                    start: srcPos,
                    end: dstPos,
                    volume: 0,
                    lastSeen: timestamp
                  };
                }

                linksRef.current[linkId].volume += Math.max(data.size, 500);
                linksRef.current[linkId].lastSeen = timestamp;
                linksRef.current[linkId].start = srcPos;
                linksRef.current[linkId].end = dstPos;
              }
            },
            error: (err) => console.error('gRPC Error:', err),
            complete: () => console.log('gRPC Stream Completed')
          });
      } catch (err) {
        console.error("Failed to fetch config", err);
      }
    };

    init();

  }, []);

  // Sync Loop
  useEffect(() => {
    const interval = setInterval(() => {
      const now = Date.now();

      // --- Update Agents ---
      const currentAgents = agentsRef.current;
      const agentIPs = Object.keys(currentAgents);
      let agentsChanged = false;

      // Update Agent Positions based on count
      if (agentIPs.length === 1) {
        // Single agent: Center
        if (currentAgents[agentIPs[0]].position.x !== 0 || currentAgents[agentIPs[0]].position.z !== 0) {
          currentAgents[agentIPs[0]].position = new THREE.Vector3(0, 2, 0);
          agentsChanged = true;
        }
      } else if (agentIPs.length > 1) {
        // Multiple agents: Distribute in circle
        agentIPs.forEach((ip, index) => {
          const angle = (index / agentIPs.length) * Math.PI * 2;
          const tx = Math.cos(angle) * AGENT_CLUSTER_RADIUS;
          const tz = Math.sin(angle) * AGENT_CLUSTER_RADIUS;
          const targetPos = new THREE.Vector3(tx, 2, tz);

          if (currentAgents[ip].position.distanceTo(targetPos) > 0.1) {
            currentAgents[ip].position = targetPos;
            agentsChanged = true;
          }
        });
      }

      // Cleanup old agents? Maybe keep them for longer than peers.
      // For now, let's just keep them.

      if (agentsChanged || Object.keys(agentsState).length !== agentIPs.length) {
        setAgentsState({ ...currentAgents });
      }


      // --- Update Peers ---
      const currentPeers = peersRef.current;
      let peersChanged = false;
      let newEffects = [];

      Object.keys(currentPeers).forEach(key => {
        currentPeers[key].volume *= 0.8;

        // Skip timeout for selected peer
        const isSelected = (key === selectedPeer);

        if (!isSelected && now - currentPeers[key].lastSeen > timeoutRef.current) {
          newEffects.push({ id: key + '-' + now, position: currentPeers[key].position });
          delete currentPeers[key];
          peersChanged = true;

          if (selectedPeer === key) setSelectedPeer(null); // Should not happen due to guard, but safety
        }
      });

      if (newEffects.length > 0) {
        setDespawnEffects(prev => [...prev, ...newEffects]);
      }

      // Ranking
      const sortedKeys = Object.keys(currentPeers).sort((a, b) => currentPeers[b].volume - currentPeers[a].volume);
      const top3 = new Set(sortedKeys.slice(0, 3));

      Object.keys(currentPeers).forEach(key => {
        const isTop = top3.has(key) && currentPeers[key].volume > 100;
        if (currentPeers[key].isHot !== isTop) {
          currentPeers[key].isHot = isTop;
          peersChanged = true;
        }
      });

      if (peersChanged) {
        setPeersState({ ...currentPeers });
      }

    }, 100);

    return () => clearInterval(interval);
  }, [selectedPeer, agentsState]); // Re-bind when needed

  // Link Sync
  useEffect(() => {
    const interval = setInterval(() => {
      const now = Date.now();
      const currentLinks = linksRef.current;

      Object.keys(currentLinks).forEach(key => {
        if (currentLinks[key].volume < 10 && (now - currentLinks[key].lastSeen > 3000)) {
          delete currentLinks[key];
        }
      });

      setLinksState(prev => {
        const prevKeys = Object.keys(prev).join(',');
        const currKeys = Object.keys(currentLinks).join(',');
        if (prevKeys !== currKeys) return { ...currentLinks };
        return prev;
      });
    }, 200);
    return () => clearInterval(interval);
  }, []);

  // Handlers
  const handlePeerClick = (ip) => {
    setSelectedPeer(ip);
    setSelectedAgent(null);
  };

  const handleAgentClick = (ip) => {
    setSelectedAgent(ip);
    setSelectedPeer(null);
  }

  const handleDragStart = (id, type) => {
    setDraggingTarget({ id, type });
    if (orbitRef.current) orbitRef.current.enabled = false;
  }

  const handleDragEnd = () => {
    setDraggingTarget(null);
    if (orbitRef.current) orbitRef.current.enabled = true;
  }

  const handleDrag = (point) => {
    if (draggingTarget) {
      if (draggingTarget.type === 'peer' && peersRef.current[draggingTarget.id]) {
        const p = peersRef.current[draggingTarget.id];
        p.position = new THREE.Vector3(point.x, point.y, point.z);
        setPeersState({ ...peersRef.current });
      }
    }
  }

  return (
    <div style={{ width: '100vw', height: '100vh', background: 'black', position: 'relative' }}>
      <Canvas>
        <PerspectiveCamera makeDefault position={[30, 20, 30]} fov={60} />
        <OrbitControls ref={orbitRef} autoRotate={false} />

        <ambientLight intensity={0.2} />
        <pointLight position={[10, 10, 10]} intensity={1} color="#00ffff" />
        <pointLight position={[-10, 10, -10]} intensity={1} color="#ff00ff" />

        <color attach="background" args={['#050510']} />
        <fog attach="fog" args={['#050510', 30, 150]} />

        <Grid position={[0, -0.1, 0]} args={[100, 100]} cellSize={2} cellThickness={1} cellColor="#0a4a4a" sectionSize={10} sectionThickness={1.5} sectionColor="#1a1a3a" fadeDistance={80} fadeStrength={1.5} infiniteGrid />

        {/* Invisible Plane for Raycasting during drag if needed, 
            but using onPointerMove on Peer itself is tricky because the mesh moves under cursor.
            Ideally we drag on a plane.
         */}
        {/* Invisible Plane for Raycasting during drag */}
        {draggingTarget && (
          <mesh visible={false} onPointerMove={(e) => handleDrag(e.point)} rotation={[-Math.PI / 2, 0, 0]} position={[0, 0, 0]}>
            <planeGeometry args={[200, 200]} />
          </mesh>
        )}

        {/* Render Agents */}

        {Object.entries(agentsState).map(([ip, agent]) => (
          <Agent
            key={ip}
            label={ip}
            position={agent.position}
            isSelected={selectedAgent === ip}
            onClick={handleAgentClick}
          />
        ))}
        {/* If no agents detected yet, show default placeholder? Or nothing? 
            Let's show a default placeholder if empty to imply system is ready. 
         */}
        {Object.keys(agentsState).length === 0 && <Agent label="Waiting..." />}


        {Object.entries(peersState).map(([ip, data]) => (
          <Peer
            key={ip}
            ip={ip}
            position={data.position}
            isHot={data.isHot}
            isSelected={ip === selectedPeer}
            onClick={handlePeerClick}
            onDragStart={(id) => handleDragStart(id, 'peer')}
            onDragEnd={handleDragEnd}
            onDrag={handleDrag}
          />
        ))}

        {despawnEffects.map(effect => (
          <DespawnEffect key={effect.id} position={effect.position} onComplete={() => removeEffect(effect.id)} />
        ))}

        {Object.entries(linksState).map(([id, linkData]) => (
          <ConnectionLink key={id} linkData={linkData} />
        ))}

      </Canvas>

      {/* UI Overlay */}
      {(selectedPeer || selectedAgent) && (
        <InfoPanel
          peerIp={selectedPeer}
          peerData={selectedPeer ? peersRef.current[selectedPeer] : null}
          agentIp={selectedAgent}
          agentData={selectedAgent ? agentsRef.current[selectedAgent] : null}
          onClose={() => { setSelectedPeer(null); setSelectedAgent(null); }}
          geoipEnabled={geoipEnabled}
          attribution={attribution}
        />
      )}
    </div>
  );
}
