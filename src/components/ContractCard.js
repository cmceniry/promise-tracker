import { useEffect, useRef, useState } from 'react';
import { Alert, Button, Card, Form } from 'react-bootstrap';

export default function ContractCard({contractId, contractFilename, contractText, contractError, updateFilename, updateContract, deleteContract}) {
  const downloadRef = useRef("");
  const [downloadLink, setDownloadLink] = useState("");
  useEffect(() => {
    const d = new Blob([contractText], { type: 'text/json' });
    if (downloadRef.current !== "") window.URL.revokeObjectURL(downloadRef.current);
    downloadRef.current = window.URL.createObjectURL(d);
    setDownloadLink(downloadRef.current);
  }, [contractText]);

  return <Card body>
    <Form>
      <Form.Control
        id={contractId}
        as="input"
        value={contractFilename}
        onChange={updateFilename}
      />
      <Form.Control
        id={contractId}
        as="textarea"
        rows="10"
        value={contractText}
        onChange={updateContract}
      />
    </Form>
    {contractError && <Alert variant="danger">{contractError}</Alert>}
    <a download={contractFilename} href={downloadLink}><Button>Download</Button></a>{' '}
    <Button id={contractId} onClick={deleteContract}>Delete</Button>
  </Card>
}
