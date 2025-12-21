import React, { useRef, useEffect, useState } from 'react';
import { Card } from 'react-bootstrap';
import ForceGraph2D from 'react-force-graph-2d';
import networkDiagram from '../libs/promise-tracker/network-diagram';
import './PromiseNetworkGraph.css';

export default function PromiseNetworkGraph({ pt, simId }) {
  const graphRef = useRef();
  const [graphData, setGraphData] = useState({ nodes: [], links: [] });
  const [dimensions, setDimensions] = useState({ width: 800, height: 600 });

  useEffect(() => {
    if (!pt || pt.is_empty()) {
      setGraphData({ nodes: [], links: [] });
      return;
    }

    const data = networkDiagram(pt);
    setGraphData(data);
  }, [pt]);

  // Handle window resize
  useEffect(() => {
    const handleResize = () => {
      if (graphRef.current) {
        const container = graphRef.current;
        setDimensions({
          width: container.clientWidth || 800,
          height: 600, // Fixed height for consistency
        });
      }
    };

    // Initial size
    handleResize();
    
    // Update on window resize
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  if (!pt) {
    return (
      <Card body className="promise-network-container">
        <div style={{ padding: '2rem', textAlign: 'center', color: '#666' }}>
          Loading...
        </div>
      </Card>
    );
  }

  if (pt.is_empty()) {
    return (
      <Card body className="promise-network-container">
        <div style={{ padding: '2rem', textAlign: 'center', color: '#666' }}>
          No contracts defined. Add contracts to see relationships.
        </div>
      </Card>
    );
  }

  if (graphData.nodes.length === 0) {
    return (
      <Card body className="promise-network-container">
        <div style={{ padding: '2rem', textAlign: 'center', color: '#666' }}>
          No relationships found.
        </div>
      </Card>
    );
  }

  return (
    <Card body className="promise-network-container">
      <div className="promise-network-legend">
        <div className="promise-network-legend-item">
          <span className="promise-network-legend-color circle" style={{ backgroundColor: '#1976D2' }}></span>
          <span>Component</span>
        </div>
        <div className="promise-network-legend-item">
          <span className="promise-network-legend-color circle" style={{ backgroundColor: '#4CAF50' }}></span>
          <span>Behavior (satisfied)</span>
        </div>
        <div className="promise-network-legend-item">
          <span className="promise-network-legend-color circle" style={{ backgroundColor: '#C62828' }}></span>
          <span>Behavior (unsatisfied)</span>
        </div>
        <div className="promise-network-legend-item">
          <span style={{ color: '#4CAF50' }}>━━</span>
          <span>Relationship (satisfied)</span>
        </div>
        <div className="promise-network-legend-item">
          <span style={{ color: '#C62828' }}>┅┅</span>
          <span>Relationship (unsatisfied)</span>
        </div>
      </div>
      <div ref={graphRef} style={{ width: '100%', height: '600px', minHeight: '400px' }}>
        <ForceGraph2D
          graphData={graphData}
          width={dimensions.width}
          height={dimensions.height}
          nodeLabel={(node) => `${node.label} (${node.type})`}
          nodeColor={(node) => {
            if (node.type === 'component') {
              return '#1976D2'; // Blue for components
            } else {
              // Green for satisfied behaviors, red if unsatisfied
              return node.satisfied ? '#4CAF50' : '#C62828';
            }
          }}
          nodeVal={(node) => {
            // Size nodes based on type
            return node.type === 'component' ? 10 : 8;
          }}
          linkColor={(link) => {
            // Green for satisfied relationships, red for unsatisfied
            return link.satisfied ? '#4CAF50' : '#C62828';
          }}
          linkWidth={(link) => {
            return link.satisfied ? 2 : 1.5;
          }}
          linkLineDash={(link) => {
            // Dashed line for unsatisfied relationships
            return !link.satisfied ? [5, 5] : null;
          }}
          linkDirectionalArrowLength={6}
          linkDirectionalArrowRelPos={1}
          linkCurvature={0.1}
          cooldownTicks={100}
          onEngineStop={() => {
            // Graph has finished positioning
          }}
        />
      </div>
    </Card>
  );
}

