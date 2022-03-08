import { useEffect, useState } from 'react';
import { Alert, Button, Card, Form } from 'react-bootstrap';

export default function ContractCard({contractId, contractFilename, contractText, contractError, updateFilename, updateContract, deleteContract}) {
  const [downloadLink, setDownloadLink] = useState("");
  const buildContractDownload = () => {
    const d = new Blob([contractText], { type: 'text/json' });
    if (downloadLink !== "") window.URL.revokeObjectURL(downloadLink);
    setDownloadLink(window.URL.createObjectURL(d));
  }
  useEffect(() => {buildContractDownload()}, [contractText]);

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
