import { Flex, ScrollArea } from "@mantine/core";
import { useViewportSize } from "@mantine/hooks";
import Clip from "./Clip";
import { useEffect, useState } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";




export default function ListOfClips() {
    const { height } = useViewportSize();

    const [clips, SetClips] = useState<Array<FastClip>>([])



    useEffect(() => {
        const unlistenPromises: Promise<UnlistenFn>[] = [];

        unlistenPromises.push(listen('update_clips', (event) => {
            console.log(event.payload);
            SetClips(event.payload as Array<FastClip>);
        }));

        return () => {
            unlistenPromises.forEach((unlistenPromise) => {
                unlistenPromise.then((unlisten) => unlisten());
            });
        };
    }, []);


    useEffect(() => {
        const initialize = async () => {
            try {
                const message = await invoke('get_clips');
                SetClips(message as Array<FastClip>);
            } catch (error) {
                console.error(error); // Log the error if something goes wrong
            }
        };

        initialize(); // Call the async function inside useEffect
    }, []); // Empty dependency array ensures it runs only on mount




    return (

        <Flex
            w="100%"
            maw="500px"
            gap="md"
            align="center"
            direction="column"
            style={{ height: `${height}px` }}
            p={10}
            pt={50}
        >
            <ScrollArea
                h={height - 80}
                p={0}
                m={0}
                w="100%">
                <Flex
                    align="center"
                    gap={10}
                    direction="column" p={0} m={0}
                >
                    {clips.map((clip, index) => (
                        <Clip key={index} fast_clip={clip} />
                    ))}

                </Flex>
            </ScrollArea>

        </Flex>

    )
}
