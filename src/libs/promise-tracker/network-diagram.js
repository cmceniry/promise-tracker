/**
 * Generates network graph data structure showing all promise relationships
 * between components and behaviors.
 * 
 * @param {PT} pt - PromiseTracker instance (from WASM)
 * @returns {Object} Graph data with nodes and links arrays
 */
export default function networkDiagram(pt) {
  if (!pt || pt.is_empty()) {
    return { nodes: [], links: [] };
  }

  const agentNames = pt.get_agent_names();
  if (agentNames.length === 0) {
    return { nodes: [], links: [] };
  }

  const nodes = [];
  const links = [];
  const nodeMap = new Map(); // Track nodes by ID to avoid duplicates

  // Helper to create or get node
  const getOrCreateNode = (id, label, type) => {
    if (!nodeMap.has(id)) {
      const node = {
        id: id,
        label: label,
        type: type, // 'component' or 'behavior'
        satisfied: true, // default, will be updated if unsatisfied
      };
      nodeMap.set(id, node);
      nodes.push(node);
    }
    return nodeMap.get(id);
  };

  // Helper to check if a link already exists (avoid duplicates)
  const linkExists = (source, target, type) => {
    return links.some(link => 
      link.source === source && 
      link.target === target && 
      link.type === type
    );
  };

  // Recursively process a resolution to extract all nested relationships
  const processResolution = (behaviorName, resolution, parentBehavior = null) => {
    const satisfied = resolution.satisfying_offers || resolution.satisfied || [];
    const unsatisfied = resolution.unsatisfying_offers || resolution.unsatisfied || [];
    
    // Process satisfied offers
    satisfied.forEach((offer) => {
      const providerName = offer.agent_name || offer.component || offer.componentName;
      if (providerName) {
        // Create link from behavior to provider (provides relationship)
        if (!linkExists(behaviorName, providerName, 'provides')) {
          links.push({
            source: behaviorName,
            target: providerName,
            type: 'provides',
            satisfied: true,
          });
        }
        
        // Process nested conditions (resolved_conditions)
        if (offer.resolved_conditions && offer.resolved_conditions.length > 0) {
          offer.resolved_conditions.forEach((condition) => {
            const conditionBehaviorName = condition.behavior_name || condition.behavior;
            if (conditionBehaviorName) {
              // Create link from provider component to condition behavior (needs condition)
              if (!linkExists(providerName, conditionBehaviorName, 'needs')) {
                links.push({
                  source: providerName,
                  target: conditionBehaviorName,
                  type: 'needs',
                  satisfied: condition.satisfying_offers && condition.satisfying_offers.length > 0,
                });
              }
              
              // Recursively process the condition's resolution
              processResolution(conditionBehaviorName, condition, behaviorName);
            }
          });
        }
      }
    });
    
    // Process unsatisfied offers
    unsatisfied.forEach((offer) => {
      const providerName = offer.agent_name || offer.component || offer.componentName;
      if (providerName) {
        // Create link from behavior to provider (unsatisfied)
        if (!linkExists(behaviorName, providerName, 'provides')) {
          links.push({
            source: behaviorName,
            target: providerName,
            type: 'provides',
            satisfied: false,
          });
        }
        
        // Process nested conditions even for unsatisfied offers
        if (offer.resolved_conditions && offer.resolved_conditions.length > 0) {
          offer.resolved_conditions.forEach((condition) => {
            const conditionBehaviorName = condition.behavior_name || condition.behavior;
            if (conditionBehaviorName) {
              // Create link from provider component to condition behavior (needs condition)
              const conditionSatisfied = condition.satisfying_offers && condition.satisfying_offers.length > 0;
              if (!linkExists(providerName, conditionBehaviorName, 'needs')) {
                links.push({
                  source: providerName,
                  target: conditionBehaviorName,
                  type: 'needs',
                  satisfied: conditionSatisfied,
                });
              } else {
                // Update existing link's satisfied status
                const existingLink = links.find(link => 
                  link.source === providerName && 
                  link.target === conditionBehaviorName && 
                  link.type === 'needs'
                );
                if (existingLink) {
                  existingLink.satisfied = conditionSatisfied;
                }
              }
              
              // Recursively process the condition's resolution
              processResolution(conditionBehaviorName, condition, behaviorName);
            }
          });
        }
      }
    });
    
    // Update behavior node satisfaction status
    const behaviorNode = nodeMap.get(behaviorName);
    if (behaviorNode) {
      if (satisfied.length === 0 && unsatisfied.length === 0) {
        behaviorNode.satisfied = false;
      } else if (unsatisfied.length > 0 && satisfied.length === 0) {
        behaviorNode.satisfied = false;
      }
    }
  };

  // Process each agent (component)
  agentNames.forEach((agentName) => {
    // Get what this agent wants
    const wants = pt.get_agent_wants(agentName);
    
    wants.forEach((wantBehavior) => {
      const behaviorNode = getOrCreateNode(wantBehavior, wantBehavior, 'behavior');
      
      // Create link from component to behavior (wants relationship)
      // We'll determine if it's satisfied after resolving
      if (!linkExists(agentName, wantBehavior, 'wants')) {
        links.push({
          source: agentName,
          target: wantBehavior,
          type: 'wants',
          satisfied: true, // Will be updated after resolution
        });
      }
      
      // Resolve the behavior to find providers and nested relationships
      try {
        const resolution = pt.resolve(wantBehavior);
        const hasSatisfiedProviders = (resolution.satisfying_offers || resolution.satisfied || []).length > 0;
        
        // Update the wants link based on whether there are satisfied providers
        const wantsLink = links.find(link => 
          link.source === agentName && 
          link.target === wantBehavior && 
          link.type === 'wants'
        );
        if (wantsLink) {
          wantsLink.satisfied = hasSatisfiedProviders;
        }
        
        processResolution(wantBehavior, resolution);
      } catch (e) {
        // If resolution fails, mark behavior as unsatisfied
        behaviorNode.satisfied = false;
        // Update wants link to unsatisfied
        const wantsLink = links.find(link => 
          link.source === agentName && 
          link.target === wantBehavior && 
          link.type === 'wants'
        );
        if (wantsLink) {
          wantsLink.satisfied = false;
        }
      }
    });
  });

  return { nodes, links };
}

