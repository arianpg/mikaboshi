import React, { useState, useEffect } from 'react';
import TrafficVisualizer from './TrafficVisualizer';
import { isWebGLAvailable } from './webglCheck';

function App() {
  const [webgl, setWebgl] = useState(true);

  useEffect(() => {
    setWebgl(isWebGLAvailable());
  }, []);

  if (!webgl) {
    return (
      <div style={{ color: 'white', display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh', flexDirection: 'column' }}>
        <h1>WebGL Not Available</h1>
        <p>Your browser does not support WebGL, which is required for 3D visualization.</p>
        <p>Please enable hardware acceleration in your browser settings.</p>
      </div>
    );
  }

  return (
    <div className="App">
      <TrafficVisualizer />
    </div>
  );
}

export default App;
