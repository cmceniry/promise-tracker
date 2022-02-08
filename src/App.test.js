import { render, screen } from '@testing-library/react';
import App from './App';

it('renderes without crashing', () => {
  render(<App />);
  const h = screen.getByRole("heading");
  expect(h).toBeInTheDocument();
})
