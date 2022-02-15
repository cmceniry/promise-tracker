import { Alert, Card, Container, Form } from 'react-bootstrap';

export default function ContractCard({contractId, contractText, contractError, updateContract}) {
  return <Card body><Container>
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
  </Container>
  </Card>
}
