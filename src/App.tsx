import { Center } from "@mantine/core";
import ListOfClips from "./Components/Viewport";
import "./App.css";
import "@mantine/core/styles.css"; 
import '@mantine/notifications/styles.css';
import ToolBar from "./Components/ToolBar";

function App() {
  return (
    <Center  className="bg-zinc-800">
      <ToolBar/>
      <ListOfClips/>
    </Center>
  );
}

export default App;
