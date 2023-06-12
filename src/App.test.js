import { render, screen } from '@testing-library/react';
import App from './App';

jest.mock('mermaid', () => ({
  initialize: jest.fn(),
  parse: jest.fn(),
  render: jest.fn(),
}));

it('renderes without crashing', () => {
  render(<App />);
  const h = screen.getByRole("heading");
  expect(h).toBeInTheDocument();
})
