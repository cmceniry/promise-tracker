import { Alert, Container, Form } from 'react-bootstrap';
import { from_yaml } from '../libs/promise-tracker/contract';

export default function ContractInput({contractText, updateContract}) {
  let error = null;
  try {
    if (contractText) {
      from_yaml(contractText);
    };
  } catch (e) {
    error = e.toString();
  };

  return <Container>
    <Form>
      <Form.Control
        as="textarea"
        rows="10"
        value={contractText}
        onChange={updateContract}
      />
    </Form>
    {error && <Alert variant="danger">{error}</Alert>}
  </Container>
}