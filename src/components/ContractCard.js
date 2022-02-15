import { Alert, Button, Card, Container, Form } from 'react-bootstrap';

export default function ContractCard({contractId, contractText, contractError, updateContract, deleteContract}) {
  return <Card body>
    <Form>
      <Form.Control
        id={contractId}
        as="textarea"
        rows="10"
        value={contractText}
        onChange={updateContract}
      />
    </Form>
    {contractError && <Alert variant="danger">{contractError}</Alert>}
    <Button id={contractId} onClick={deleteContract}>Delete</Button>
  </Card>
}
