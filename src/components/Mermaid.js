import React, { useEffect, useRef, useState } from "react";
import mermaid from "mermaid";

export default function Mermaid({chart, id}) {
  const chartId = id === undefined ? "promisechart" : id
  const [rendering, setRendering] = useState("");
  const [error, setError] = useState(null);
  const [isLoading, setIsLoading] = useState(true);
  const containerRef = useRef(null);

  useEffect(() => {
    if (!chart) {
      setRendering("");
      setError(null);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      // Initialize mermaid with settings for sequence diagrams
      mermaid.initialize({
        startOnLoad: false,
        theme: 'default',
        sequence: {
          useMaxWidth: false,
        },
        securityLevel: 'loose',
      });

      // Parse the chart to check for syntax errors
      mermaid.parse(chart);
      
      // Render the chart - handle both sync and async versions
      const renderResult = mermaid.render(chartId, chart);
      
      // Check if render returns a promise (newer versions) or result directly (older versions)
      if (renderResult && typeof renderResult.then === 'function') {
        // Async version - returns a promise
        renderResult.then((result) => {
          // Result might be an object with .svg property or a string
          const svg = typeof result === 'string' ? result : (result.svg || result);
          setRendering(svg);
          setIsLoading(false);
        }).catch((e) => {
          console.error("Mermaid rendering error:", e);
          setError(e.message || "Failed to render diagram");
          setIsLoading(false);
        });
      } else {
        // Sync version - returns string or object with .svg property
        const svg = typeof renderResult === 'string' ? renderResult : (renderResult?.svg || renderResult || '');
        setRendering(svg);
        setIsLoading(false);
      }
    } catch (e) {
      console.error("Mermaid parsing error:", e);
      setError(e.message || "Invalid diagram syntax");
      setIsLoading(false);
    }
  }, [chart, chartId]);

  if (isLoading) {
    return (
      <div style={{overflow: "auto", padding: "1rem", textAlign: "center"}}>
        <div>Loading diagram...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div style={{overflow: "auto", padding: "1rem"}}>
        <div style={{color: "red", marginBottom: "0.5rem"}}>Error rendering diagram:</div>
        <div style={{fontSize: "0.9em", color: "#666"}}>{error}</div>
        {chart && (
          <details style={{marginTop: "1rem"}}>
            <summary style={{cursor: "pointer"}}>Show chart source</summary>
            <pre style={{background: "#f5f5f5", padding: "0.5rem", marginTop: "0.5rem", overflow: "auto", fontSize: "0.8em"}}>
              {chart}
            </pre>
          </details>
        )}
      </div>
    );
  }

  if (!rendering) {
    return (
      <div style={{overflow: "auto", padding: "1rem", textAlign: "center", color: "#666"}}>
        No diagram to display
      </div>
    );
  }

  return (
    <div 
      ref={containerRef}
      style={{
        overflow: "auto",
        width: "100%",
        minHeight: "200px",
      }}
      dangerouslySetInnerHTML={{__html: rendering}}
    />
  );
}
