import { Center } from "@mantine/core";
import ListOfClips from "./Components/Viewport";
import "./App.css";
import "@mantine/core/styles.css"; 
import '@mantine/notifications/styles.css';
import New from "./Components/NewClip";
function App() {

  return (
    <Center className="bg-zinc-800">
      <ListOfClips/>
      <New/>
    </Center>
  );
}

export default App;
