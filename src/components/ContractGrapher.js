import React from "react";
import { useState, useEffect } from 'react';
import Mermaid from './Mermaid';
import { Form } from 'react-bootstrap';
import { allFromYAML } from '../libs/promise-tracker/contract';
import PromiseTracker from '../libs/promise-tracker/promise-tracker';
import ptdiagram from '../libs/promise-tracker/diagram';

export default function ContractGrapher({contracts}) {
    const [diagram, setDiagram] = useState("sequenceDiagram\nyou->>contract: enter something");
    const [dComponent, setDComponent] = useState("");
    const [dBehavior, setDBehavior] = useState("");

    useEffect(() => {
        try {
            if (contracts.length === 0) {
                setDiagram("sequenceDiagram\nyou->>contract: enter something");
                return;
            }
            if (contracts.filter((c) => c.err).length > 0) {
                return;
            }
            if (dComponent === null || dComponent === "") {
                setDiagram("sequenceDiagram\nyou->>component: enter something");
                return;
            }
            if (dBehavior === null || dBehavior === "") {
                setDiagram("sequenceDiagram\nyou->>behavior: enter something");
                return;
            }
            const pt = new PromiseTracker();
            contracts.forEach((c) => {
                if (c.text) {
                    allFromYAML(c.text).forEach((comp) => pt.addComponent(comp));
                }
            });
            if (!pt.getBehaviorNames().includes(dBehavior)) {
                setDiagram("sequenceDiagram\nyou->>behavior: enter a valid behacvior");
                return;
            }
            setDiagram(ptdiagram({...pt.resolve(dBehavior), component: dComponent}));
            } catch {console.log("stuff")};
        }, [contracts, dComponent, dBehavior]);
    
    const updateDComponent = (e) => {
        e.preventDefault();
        setDComponent(e.target.value);
    };
    
    const updateDBehavior = (e) => {
        e.preventDefault();
        setDBehavior(e.target.value);
    };    

    return <>
            <Form>
              <Form.Control type="text" placeholder="Component" value={dComponent} onChange={updateDComponent} />
            {/* </Form>
            <Form> */}
              <Form.Control type="text" placeholder="Behavior" value={dBehavior} onChange={updateDBehavior} />
            </Form>
            <Mermaid chart={diagram}></Mermaid>
        </>
}