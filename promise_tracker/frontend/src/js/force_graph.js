// Force-directed graph visualization using D3-force
// This module handles rendering promise network graphs

// Store active simulations for cleanup
const activeSimulations = new Map();

// Colors matching the React implementation
const COLORS = {
    component: '#1976D2',    // Blue for components
    satisfied: '#4CAF50',    // Green for satisfied
    unsatisfied: '#C62828',  // Red for unsatisfied
};

/**
 * Create and render a force-directed graph
 * @param {string} containerId - ID of the container element
 * @param {Array} nodes - Array of node objects {id, label, type, satisfied}
 * @param {Array} links - Array of link objects {source, target, type, satisfied}
 */
export function create_force_graph(containerId, nodes, links) {
    const container = document.getElementById(containerId);
    if (!container) {
        console.error(`Container ${containerId} not found`);
        return;
    }

    // Clean up any existing simulation
    destroy_force_graph(containerId);

    // Clear the container
    container.innerHTML = '';

    // If no nodes, show a message
    if (!nodes || nodes.length === 0) {
        container.innerHTML = '<div style="padding: 2rem; text-align: center; color: #666;">No relationships found.</div>';
        return;
    }

    // Create canvas
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');

    // Set canvas size
    const rect = container.getBoundingClientRect();
    const width = rect.width || 800;
    const height = 400;

    canvas.width = width;
    canvas.height = height;
    canvas.style.display = 'block';
    canvas.style.width = '100%';
    canvas.style.height = `${height}px`;
    container.appendChild(canvas);

    // Parse nodes and links - create deep copies and convert IDs to objects
    const nodeMap = new Map();
    const graphNodes = nodes.map(n => {
        const node = { ...n, x: width / 2, y: height / 2 };
        nodeMap.set(n.id, node);
        return node;
    });

    const graphLinks = links.map(l => ({
        ...l,
        source: nodeMap.get(l.source) || l.source,
        target: nodeMap.get(l.target) || l.target,
    }));

    // Create D3 force simulation
    const simulation = d3.forceSimulation(graphNodes)
        .force('link', d3.forceLink(graphLinks)
            .id(d => d.id)
            .distance(100)
            .strength(0.5))
        .force('charge', d3.forceManyBody()
            .strength(-300))
        .force('center', d3.forceCenter(width / 2, height / 2))
        .force('collision', d3.forceCollide()
            .radius(30))
        .alphaDecay(0.02);

    // Store the simulation for later cleanup
    activeSimulations.set(containerId, { simulation, canvas });

    // Render function
    function render() {
        ctx.clearRect(0, 0, width, height);

        // Draw links
        graphLinks.forEach(link => {
            if (!link.source.x || !link.target.x) return;

            ctx.beginPath();
            ctx.moveTo(link.source.x, link.source.y);
            ctx.lineTo(link.target.x, link.target.y);

            ctx.strokeStyle = link.satisfied ? COLORS.satisfied : COLORS.unsatisfied;
            ctx.lineWidth = link.satisfied ? 2 : 1.5;

            // Dashed line for unsatisfied
            if (!link.satisfied) {
                ctx.setLineDash([5, 5]);
            } else {
                ctx.setLineDash([]);
            }

            ctx.stroke();
            ctx.setLineDash([]);

            // Draw arrow
            const dx = link.target.x - link.source.x;
            const dy = link.target.y - link.source.y;
            const angle = Math.atan2(dy, dx);
            const nodeRadius = link.target.type === 'component' ? 12 : 10;
            const arrowX = link.target.x - nodeRadius * Math.cos(angle);
            const arrowY = link.target.y - nodeRadius * Math.sin(angle);

            const arrowLength = 8;
            const arrowWidth = Math.PI / 6;

            ctx.beginPath();
            ctx.moveTo(arrowX, arrowY);
            ctx.lineTo(
                arrowX - arrowLength * Math.cos(angle - arrowWidth),
                arrowY - arrowLength * Math.sin(angle - arrowWidth)
            );
            ctx.lineTo(
                arrowX - arrowLength * Math.cos(angle + arrowWidth),
                arrowY - arrowLength * Math.sin(angle + arrowWidth)
            );
            ctx.closePath();
            ctx.fillStyle = link.satisfied ? COLORS.satisfied : COLORS.unsatisfied;
            ctx.fill();
        });

        // Draw nodes
        graphNodes.forEach(node => {
            if (!node.x || !node.y) return;

            const radius = node.type === 'component' ? 12 : 10;

            // Node color based on type and satisfaction
            let color;
            if (node.type === 'component') {
                color = COLORS.component;
            } else {
                color = node.satisfied ? COLORS.satisfied : COLORS.unsatisfied;
            }

            ctx.beginPath();
            ctx.arc(node.x, node.y, radius, 0, 2 * Math.PI);
            ctx.fillStyle = color;
            ctx.fill();
            ctx.strokeStyle = '#fff';
            ctx.lineWidth = 2;
            ctx.stroke();

            // Draw label
            ctx.fillStyle = '#333';
            ctx.font = '11px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillText(node.label, node.x, node.y + radius + 4);
        });
    }

    // Update on each tick
    simulation.on('tick', render);

    // Handle resize
    function handleResize() {
        const newRect = container.getBoundingClientRect();
        const newWidth = newRect.width || 800;

        if (Math.abs(newWidth - canvas.width) > 10) {
            canvas.width = newWidth;
            canvas.style.width = '100%';
            simulation.force('center', d3.forceCenter(newWidth / 2, height / 2));
            simulation.alpha(0.3).restart();
        }
    }

    // Add resize observer
    const resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(container);

    // Store resize observer for cleanup
    const stored = activeSimulations.get(containerId);
    if (stored) {
        stored.resizeObserver = resizeObserver;
    }

    // Initial render
    render();
}

/**
 * Destroy a force graph and clean up resources
 * @param {string} containerId - ID of the container element
 */
export function destroy_force_graph(containerId) {
    const stored = activeSimulations.get(containerId);
    if (stored) {
        if (stored.simulation) {
            stored.simulation.stop();
        }
        if (stored.resizeObserver) {
            stored.resizeObserver.disconnect();
        }
        activeSimulations.delete(containerId);
    }
}

/**
 * Update an existing force graph with new data
 * @param {string} containerId - ID of the container element
 * @param {Array} nodes - Array of node objects
 * @param {Array} links - Array of link objects
 */
export function update_force_graph(containerId, nodes, links) {
    // For simplicity, just recreate the graph
    create_force_graph(containerId, nodes, links);
}
