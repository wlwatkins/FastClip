import { Button, Flex, ScrollArea } from "@mantine/core";
import { useViewportSize } from "@mantine/hooks";
import Clip from "./Clip";
import { useEffect, useState } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import FastClip from "../Classes/FastClip";
import { invoke } from "@tauri-apps/api/core";
import { notifications } from '@mantine/notifications';


export default function ListOfClips() {
    const { height } = useViewportSize();

    const [clips, SetClips] = useState<Array<FastClip>>([])



    useEffect(() => {
        const unlistenPromises: Promise<UnlistenFn>[] = [];

        unlistenPromises.push(listen('update_clips', (event) => {
            console.log(event.payload as string);
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
            invoke('get_clips')
                .then((message) => {
                    console.log(message);
                    SetClips(message as Array<FastClip>);
                })
                .catch((error) => {
                    console.error(error);
                });

        };

        initialize(); // Call the async function inside useEffect
    }, []); // Empty dependency array ensures it runs only on mount




    return (

        <Flex
            w="95%"
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
                m={0} scrollbars="y"
                w="100%">
                <Flex
                    align="space-around"
                    gap={10}
                    direction="column" p={0} m={0}
                >
                    {clips.map((clip) => (
                        <Clip key={clip.id} fast_clip={clip} /> // Assuming `clip.id` is unique
                    ))}


                </Flex>
            </ScrollArea>

          
        </Flex>

    )
}
