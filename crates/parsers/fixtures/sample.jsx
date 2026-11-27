export function FunctionComponent({ title }) {
  return <section><h1>{title}</h1></section>;
}

export const ArrowComponent = ({ items }) => (
  <ul>{items.map((item) => <li key={item.id}>{item.label}</li>)}</ul>
);

export class ClassComponent extends React.Component {
  handleClick = () => {};
  render() {
    return (
      <main>
        <FunctionComponent title="Hello" />
        <ArrowComponent items={[]} />
      </main>
    );
  }
}
